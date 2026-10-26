//! `TextStyle`, adapted from Compose's `TextStyleTest`.

use std::hash::{Hash, Hasher};

use compose_ui::{
    BaselineShift, Brush, Color, DrawStyle, EmojiSupportMatch, FontFamily, FontStyle,
    FontSynthesis, FontWeight, Hyphens, LayoutDirection, LineBreak, LineBreakStrategy,
    LineBreakStrictness, LineBreakWordBreak, LineHeightAlignment, LineHeightStyle, LineHeightTrim,
    LocaleList, Offset, ParagraphStyle, PlatformTextStyle, Shadow, SpanStyle, Stroke, TextAlign,
    TextDecoration, TextDirection, TextGeometricTransform, TextIndent, TextMotion, TextStyle,
    TextUnit, UnitExt,
};

fn empty() -> TextStyle {
    TextStyle::DEFAULT
}

fn gradient() -> Brush {
    Brush::linear_gradient([Color::BLUE, Color::RED])
}

fn hash_of(style: &TextStyle) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    style.hash(&mut hasher);
    hasher.finish()
}

fn indent(first_line: TextUnit) -> TextIndent {
    TextIndent { first_line, ..TextIndent::NONE }
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, constructor with default values
#[test]
fn a_default_text_style_sets_nothing() {
    let style = TextStyle::default();
    assert_eq!(style.foreground().brush(), None);
    assert_eq!(style.foreground().color(), Color::UNSPECIFIED);
    assert!(style.font_size.is_unspecified() && style.letter_spacing.is_unspecified());
    assert_eq!((style.font_weight, style.font_style, style.font_family), (None, None, None));
    assert_eq!(style.locale_list, None);
    assert_eq!(style.background, Color::UNSPECIFIED);
    assert_eq!((style.draw_style, style.text_decoration), (None, None));
    assert_eq!((style.text_align, style.text_direction), (None, None));
    assert_eq!(style.platform_style, None);
    assert_eq!((style.line_break, style.hyphens, style.text_motion), (None, None, None));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, constructor with customized brush
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, constructor with customized brush and alpha
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, constructor with gradient brush has unspecified color
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, constructor with SolidColor converts to regular color
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, constructor with customized color
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, constructor with half-transparent color
#[test]
fn the_paint_is_the_brush_if_any_else_the_color() {
    let style = TextStyle { brush: Some(gradient()), alpha: 0.3, ..empty() };
    assert_eq!(style.foreground().brush(), Some(&gradient()));
    assert_eq!(style.foreground().alpha(), 0.3);
    assert_eq!(style.foreground().color(), Color::UNSPECIFIED);
    let style = TextStyle { brush: Some(Brush::solid(Color::RED)), ..empty() };
    assert_eq!((style.foreground().color(), style.foreground().brush()), (Color::RED, None));
    let style = TextStyle { color: Color::RED, ..empty() };
    assert_eq!(style.foreground().color(), Color::RED);
    let half = Color::RED.with_alpha(0.5);
    let style = TextStyle { color: half, ..empty() };
    assert_eq!(style.foreground().color(), half);
    assert!((style.foreground().alpha() - 0.5).abs() < 1.0 / 256.0);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, constructor with customized hyphens
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, constructor with customized textMotion
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, constructor with customized fontSize
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, constructor with customized fontWeight
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, constructor with customized fontStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, constructor with customized letterSpacing
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, constructor with customized baselineShift
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, constructor with customized locale
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, constructor with customized background
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, constructor with customized textDecoration
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, constructor with customized fontFamily
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, constructor with customized lineBreak
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, constructor with customized drawStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, constructor without platformStyle sets platformStyle to null
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, constructor without lineHeightStyle sets lineHeightStyle to null
#[test]
fn a_text_style_keeps_each_field_it_is_given() {
    let style = TextStyle {
        hyphens: Some(Hyphens::Auto),
        text_motion: Some(TextMotion::Animated),
        font_size: 18.sp(),
        font_weight: Some(FontWeight::W500),
        font_style: Some(FontStyle::Italic),
        letter_spacing: 1.em(),
        baseline_shift: Some(BaselineShift::SUPERSCRIPT),
        locale_list: Some(LocaleList::from_language_tags("en-US")),
        background: Color::RED,
        text_decoration: Some(TextDecoration::UNDERLINE),
        font_family: Some(FontFamily::SansSerif),
        line_break: Some(LineBreak::HEADING),
        draw_style: Some(DrawStyle::Stroke(Stroke::new(8.0))),
        text_align: Some(TextAlign::Start),
        ..empty()
    };
    assert_eq!(
        (style.hyphens, style.text_motion),
        (Some(Hyphens::Auto), Some(TextMotion::Animated))
    );
    assert_eq!((style.font_size, style.letter_spacing), (18.sp(), 1.em()));
    assert_eq!(
        (style.font_weight, style.font_style),
        (Some(FontWeight::W500), Some(FontStyle::Italic))
    );
    assert_eq!(style.baseline_shift, Some(BaselineShift::SUPERSCRIPT));
    assert_eq!(style.locale_list, Some(LocaleList::from_language_tags("en-US")));
    assert_eq!(style.background, Color::RED);
    assert_eq!(style.text_decoration, Some(TextDecoration::UNDERLINE));
    assert_eq!(style.font_family, Some(FontFamily::SansSerif));
    assert_eq!(style.line_break, Some(LineBreak::HEADING));
    assert_eq!(style.draw_style, Some(DrawStyle::Stroke(Stroke::new(8.0))));
    assert_eq!((style.platform_style, style.line_height_style), (None, None));
    assert_eq!(TextStyle { hyphens: Some(Hyphens::Auto), ..empty() }.line_break, None);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, empty copy with existing brush should not remove brush
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, empty copy with existing color should not remove color
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, empty copy with existing drawStyle should not remove drawStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, platformTextStyle copy with existing drawStyle should not remove drawStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, empty copy with existing hyphens should not remove hyphens
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, copy with hyphens returns new hyphens
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, copy with textMotion returns new textMotion
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, copy with lineBreak returns new lineBreak
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, copy without lineBreak uses existing lineBreak
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, copy without platformStyle uses existing platformStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, copy with lineHeightStyle returns new lineHeightStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, copy without lineHeightStyle uses existing lineHeightStyle
#[test]
fn struct_update_copies_with_one_field_replaced() {
    let bottom = LineHeightStyle::new(LineHeightAlignment::BOTTOM, LineHeightTrim::None);
    let centered = LineHeightStyle::new(LineHeightAlignment::CENTER, LineHeightTrim::Both);
    let style = TextStyle {
        brush: Some(gradient()),
        draw_style: Some(DrawStyle::Stroke(Stroke::new(2.0))),
        hyphens: Some(Hyphens::Auto),
        text_motion: Some(TextMotion::Animated),
        line_break: Some(LineBreak::PARAGRAPH),
        platform_style: Some(PlatformTextStyle::with_include_font_padding(false)),
        line_height_style: Some(bottom),
        ..empty()
    };
    let copy = style.clone();
    assert_eq!(copy.brush, Some(gradient()));
    assert_eq!(copy.draw_style, Some(DrawStyle::Stroke(Stroke::new(2.0))));
    assert_eq!(copy.hyphens, Some(Hyphens::Auto));
    assert_eq!(copy.line_break, Some(LineBreak::PARAGRAPH));
    assert_eq!(copy.platform_style, style.platform_style);
    assert_eq!(copy.line_height_style, Some(bottom));
    assert_eq!(TextStyle { color: Color::RED, ..empty() }.clone().color, Color::RED);
    let platform =
        TextStyle { platform_style: Some(PlatformTextStyle::default()), ..style.clone() };
    assert_eq!(platform.draw_style, Some(DrawStyle::Stroke(Stroke::new(2.0))));
    assert_eq!(
        TextStyle { hyphens: Some(Hyphens::None), ..style.clone() }.hyphens,
        Some(Hyphens::None)
    );
    let still = TextStyle { text_motion: Some(TextMotion::Static), ..style.clone() };
    assert_eq!(still.text_motion, Some(TextMotion::Static));
    let heading = TextStyle { line_break: Some(LineBreak::HEADING), ..style.clone() };
    assert_eq!(heading.line_break, Some(LineBreak::HEADING));
    let other = TextStyle { line_height_style: Some(centered), ..style };
    assert_eq!(other.line_height_style, Some(centered));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, brush copy with existing color should remove color
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, color copy with existing brush should remove brush
#[test]
fn replacing_the_paint_replaces_color_and_brush() {
    let style = TextStyle { color: Color::RED, ..empty() }.with_brush(Some(gradient()), f32::NAN);
    assert_eq!(
        (style.foreground().color(), style.brush.clone()),
        (Color::UNSPECIFIED, Some(gradient()))
    );
    let style = TextStyle { brush: Some(gradient()), ..empty() }.with_color(Color::RED);
    assert_eq!((style.foreground().color(), style.brush), (Color::RED, None));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with empty other should return this
#[test]
fn merging_nothing_changes_nothing() {
    assert_eq!(empty().merge(&empty()), empty());
    let style = TextStyle { color: Color::RED, line_height: 3.sp(), ..empty() };
    assert_eq!(style.merge(&empty()), style);
}

/// Merges styles made by `with`: `other`'s field when set, else this one's, else unset.
fn check_merge<T: Clone + PartialEq + std::fmt::Debug>(
    with: impl Fn(Option<T>) -> TextStyle,
    get: impl Fn(&TextStyle) -> Option<T>,
    this: T,
    other: T,
) {
    let (some_this, some_other) = (|| with(Some(this.clone())), || with(Some(other.clone())));
    assert_eq!(get(&some_this().merge(&some_other())), Some(other.clone()), "other's wins");
    assert_eq!(get(&some_this().merge(&with(None))), Some(this.clone()), "unset keeps this");
    assert_eq!(get(&with(None).merge(&some_other())), Some(other.clone()), "unset takes other's");
    assert_eq!(get(&with(None).merge(&with(None))), None, "both unset stay unset");
}

fn unit(value: Option<TextUnit>) -> TextUnit {
    value.unwrap_or(TextUnit::UNSPECIFIED)
}

fn specified(value: TextUnit) -> Option<TextUnit> {
    Some(value).filter(|value| value.is_specified())
}

fn color(value: Option<Color>) -> Color {
    value.unwrap_or(Color::UNSPECIFIED)
}

fn color_set(value: Color) -> Option<Color> {
    Some(value).filter(|value| value.is_specified())
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's hyphens is unspecified should use this hyphens
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's hyphens is set should use other's hyphens
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's color is unspecified should use this' color
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's color is set should use other's color
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's fontFamily is null should use this' fontFamily
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's fontFamily is set should use other's fontFamily
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's fontSize is unspecified should use this' fontSize
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's fontSize is set should use other's fontSize
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's fontWeight is null should use this' fontWeight
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's fontWeight is set should use other's fontWeight
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's fontStyle is null should use this' fontStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's fontStyle is set should use other's fontStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's fontSynthesis is null should use this' fontSynthesis
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's fontSynthesis is set should use other's fontSynthesis
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's fontFeature is null should use this' fontSynthesis
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's fontFeature is set should use other's fontSynthesis
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's letterSpacing is unspecified should use this' letterSpacing
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's letterSpacing is set should use other's letterSpacing
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's baselineShift is null should use this' baselineShift
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's baselineShift is set should use other's baselineShift
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's background is null should use this' background
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's background is set should use other's background
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's textDecoration is null should use this' textDecoration
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's textDecoration is set should use other's textDecoration
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's drawStyle is null should use this' drawStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's drawStyle is set should use other's drawStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's locale is null should use this' locale
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with other's locale is set should use other's locale
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge textAlign uses other's textAlign
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge textAlign other unspecified, return original
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge textAlign both unspecified returns unspecified
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge textDirection uses other's textDirection
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge textDirection other unspecified, returns original
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge textDirection both unspecified returns unspecified
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge lineHeight uses other's lineHeight
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge lineHeight other unspecified, return original
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge lineHeight both unspecified returns unspecified
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge textIndent uses other's textIndent
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge textIndent other null, return original
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge textIndent both null returns null
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with null platformStyles null has null platformStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge unspecified linebreak and lineBreak uses other's lineBreak
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge linebreak and unspecified lineBreak uses original
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with both unspecified lineBreak uses unspecified
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with both non-null lineBreak uses other's lineBreak
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge null and non-null textMotion uses other's textMotion
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge non-null and null textMotion uses original
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with both null textMotion uses null
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with both non-null textMotion uses other's textMotion
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with null lineHeightStyle uses other's lineHeightStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with non-null lineHeightStyle, returns original
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with both null lineHeightStyle returns null
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with both non-null lineHeightStyle returns other's lineHeightStyle
#[test]
fn merging_takes_each_field_from_other_when_set() {
    check_merge(
        |v| TextStyle { hyphens: v, ..empty() },
        |s| s.hyphens,
        Hyphens::Auto,
        Hyphens::None,
    );
    check_merge(
        |v| TextStyle { color: color(v), ..empty() },
        |s| color_set(s.color),
        Color::RED,
        Color::GREEN,
    );
    check_merge(
        |v| TextStyle { font_family: v, ..empty() },
        |s| s.font_family,
        FontFamily::SansSerif,
        FontFamily::Serif,
    );
    check_merge(
        |v| TextStyle { font_size: unit(v), ..empty() },
        |s| specified(s.font_size),
        3.5.sp(),
        8.7.sp(),
    );
    check_merge(
        |v| TextStyle { font_weight: v, ..empty() },
        |s| s.font_weight,
        FontWeight::W300,
        FontWeight::W500,
    );
    check_merge(
        |v| TextStyle { font_style: v, ..empty() },
        |s| s.font_style,
        FontStyle::Italic,
        FontStyle::Normal,
    );
    check_merge(
        |v| TextStyle { font_synthesis: v, ..empty() },
        |s| s.font_synthesis,
        FontSynthesis::Style,
        FontSynthesis::Weight,
    );
    check_merge(
        |v: Option<compose_ui::SharedString>| TextStyle { font_feature_settings: v, ..empty() },
        |s| s.font_feature_settings.clone(),
        "\"kern\" 0".into(),
        "\"kern\" 1".into(),
    );
    check_merge(
        |v| TextStyle { letter_spacing: unit(v), ..empty() },
        |s| specified(s.letter_spacing),
        1.2.em(),
        1.5.em(),
    );
    check_merge(
        |v| TextStyle { baseline_shift: v, ..empty() },
        |s| s.baseline_shift,
        BaselineShift::SUPERSCRIPT,
        BaselineShift::SUBSCRIPT,
    );
    check_merge(
        |v| TextStyle { background: color(v), ..empty() },
        |s| color_set(s.background),
        Color::RED,
        Color::GREEN,
    );
    check_merge(
        |v| TextStyle { text_decoration: v, ..empty() },
        |s| s.text_decoration,
        TextDecoration::LINE_THROUGH,
        TextDecoration::UNDERLINE,
    );
    check_merge(
        |v| TextStyle { draw_style: v, ..empty() },
        |s| s.draw_style,
        DrawStyle::Stroke(Stroke::default()),
        DrawStyle::Fill,
    );
    check_merge(
        |v| TextStyle { locale_list: v, ..empty() },
        |s| s.locale_list.clone(),
        LocaleList::from_language_tags("en-US"),
        LocaleList::from_language_tags("ja-JP"),
    );
    check_merge(
        |v| TextStyle { text_align: v, ..empty() },
        |s| s.text_align,
        TextAlign::Justify,
        TextAlign::Right,
    );
    check_merge(
        |v| TextStyle { text_direction: v, ..empty() },
        |s| s.text_direction,
        TextDirection::Rtl,
        TextDirection::Ltr,
    );
    check_merge(
        |v| TextStyle { line_height: unit(v), ..empty() },
        |s| specified(s.line_height),
        12.sp(),
        20.sp(),
    );
    check_merge(
        |v| TextStyle { text_indent: v, ..empty() },
        |s| s.text_indent,
        indent(12.sp()),
        indent(20.sp()),
    );
    check_merge(
        |v| TextStyle { line_break: v, ..empty() },
        |s| s.line_break,
        LineBreak::PARAGRAPH,
        LineBreak::HEADING,
    );
    check_merge(
        |v| TextStyle { text_motion: v, ..empty() },
        |s| s.text_motion,
        TextMotion::Animated,
        TextMotion::Static,
    );
    check_merge(
        |v| TextStyle { line_height_style: v, ..empty() },
        |s| s.line_height_style,
        LineHeightStyle::new(LineHeightAlignment::CENTER, LineHeightTrim::None),
        LineHeightStyle::new(LineHeightAlignment::BOTTOM, LineHeightTrim::Both),
    );
    assert_eq!(empty().merge(&empty()).platform_style, None);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with brush has other brush and no color
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge with unspecified brush has original brush
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge brush with brush uses other's alpha
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, merge brush with brush uses current alpha if other's is NaN
#[test]
fn merging_a_brush_wins_over_a_color_and_keeps_an_alpha() {
    let red = TextStyle { color: Color::RED, ..empty() };
    let brushed = TextStyle { brush: Some(gradient()), ..empty() };
    let merged = red.merge(&brushed);
    assert_eq!((merged.color, merged.brush), (Color::UNSPECIFIED, Some(gradient())));
    let merged = brushed.merge(&empty());
    assert_eq!((merged.color, merged.brush), (Color::UNSPECIFIED, Some(gradient())));
    let faint = TextStyle { alpha: 0.3, ..brushed.clone() };
    let merged = faint.merge(&TextStyle { alpha: 0.6, ..brushed.clone() });
    assert_eq!((merged.color, merged.alpha), (Color::UNSPECIFIED, 0.6));
    let merged = faint.merge(&brushed);
    assert_eq!((merged.brush, merged.alpha), (Some(gradient()), 0.3));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, plus operator merges other TextStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, plus operator merges other SpanStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, plus operator merges other ParagraphStyle
#[test]
fn plus_merges_text_span_and_paragraph_styles() {
    let style = TextStyle {
        color: Color::RED,
        font_weight: Some(FontWeight::BOLD),
        text_align: Some(TextAlign::Center),
        text_direction: Some(TextDirection::Rtl),
        ..empty()
    } + TextStyle {
        color: Color::GREEN,
        font_family: Some(FontFamily::Cursive),
        text_align: Some(TextAlign::Justify),
        line_height: 12.sp(),
        ..empty()
    };
    assert_eq!(
        style,
        TextStyle {
            color: Color::GREEN,
            font_weight: Some(FontWeight::BOLD),
            font_family: Some(FontFamily::Cursive),
            text_align: Some(TextAlign::Justify),
            text_direction: Some(TextDirection::Rtl),
            line_height: 12.sp(),
            ..empty()
        }
    );
    let style = TextStyle {
        color: Color::RED,
        font_weight: Some(FontWeight::BOLD),
        text_align: Some(TextAlign::Center),
        ..empty()
    } + SpanStyle {
        color: Color::GREEN,
        font_family: Some(FontFamily::Cursive),
        ..SpanStyle::DEFAULT
    };
    assert_eq!(
        style,
        TextStyle {
            color: Color::GREEN,
            font_weight: Some(FontWeight::BOLD),
            font_family: Some(FontFamily::Cursive),
            text_align: Some(TextAlign::Center),
            ..empty()
        }
    );
    let style = TextStyle {
        color: Color::RED,
        text_align: Some(TextAlign::Center),
        text_direction: Some(TextDirection::Rtl),
        ..empty()
    } + ParagraphStyle {
        text_align: Some(TextAlign::Justify),
        line_height: 12.sp(),
        ..ParagraphStyle::DEFAULT
    };
    assert_eq!(
        style,
        TextStyle {
            color: Color::RED,
            text_align: Some(TextAlign::Justify),
            text_direction: Some(TextDirection::Rtl),
            line_height: 12.sp(),
            ..empty()
        }
    );
}

/// Lerps styles made by `with` from `start` to `stop`: `start`'s below half way, `stop`'s from
/// there.
fn check_discrete_lerp<T: Clone + PartialEq + std::fmt::Debug>(
    with: impl Fn(Option<T>) -> TextStyle,
    get: impl Fn(&TextStyle) -> Option<T>,
    start: Option<T>,
    stop: Option<T>,
) {
    let lerped = |t| get(&TextStyle::lerp(&with(start.clone()), &with(stop.clone()), t));
    assert_eq!(lerped(0.2), start);
    assert_eq!(lerped(0.4), start);
    assert_eq!(lerped(0.6), stop);
    assert_eq!(lerped(0.8), stop);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp hyphens with a and b are not Null and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp hyphens with a and b are not Null and t is larger than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp fontFamily with a and b are not Null and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp fontFamily with a and b are not Null and t is larger than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp fontStyle with a and b are not Null and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp fontStyle with a and b are not Null and t is larger than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp fontSynthesis with a and b are not Null and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp fontSynthesis with a and b are not Null and t is larger than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp fontFeatureSettings with a and b are not Null and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp fontFeatureSettings with a and b are not Null and t is larger than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp locale with a and b are not Null and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp locale with a and b are not Null and t is larger than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp textDecoration with a and b are not Null and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp textDecoration with a and b are not Null and t is larger than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp drawStyle with a and b are not null and fraction is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp drawStyle with a and b are not null and fraction is larger than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp textAlign with a unspecified, b not null and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp textAlign with a and b are not Null and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp textAlign with a and b are not Null and t is larger than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp textDirection with a unspecified, b not null and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp textDirection with a and b are not Null and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp textDirection with a and b are not Null and t is larger than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp with non-null start, unspecified end, closer to start has non-null lineBreak
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp with non-null start, unspecified end, closer to end has unspecified lineBreak
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp with unspecified start, non-null end, closer to start has null lineBreak
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp with unspecified start, non-null end, closer to end has non-null lineBreak
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp with non-null start, null end, closer to start has non-null textMotion
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp with non-null start, null end, closer to end has null textMotion
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp with null start, non-null end, closer to start has null textMotion
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp with null start, non-null end, closer to end has non-null textMotion
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lineHeightStyle lerp with null lineHeightStyles has null lineHeightStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lineHeightStyle lerp with non-null start, null end, closer to start has non-null
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lineHeightStyle lerp with non-null start, null end, closer to end has null
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lineHeightStyle lerp with null start, non-null end, closer to start has null
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lineHeightStyle lerp with null start, non-null end, closer to end has non-null
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp with null platformStyles has null platformStyle
#[test]
fn lerping_discrete_fields_switches_half_way() {
    check_discrete_lerp(
        |v| TextStyle { hyphens: v, ..empty() },
        |s| s.hyphens,
        Some(Hyphens::Auto),
        Some(Hyphens::None),
    );
    check_discrete_lerp(
        |v| TextStyle { font_family: v, ..empty() },
        |s| s.font_family,
        Some(FontFamily::SansSerif),
        Some(FontFamily::Serif),
    );
    check_discrete_lerp(
        |v| TextStyle { font_style: v, ..empty() },
        |s| s.font_style,
        Some(FontStyle::Italic),
        Some(FontStyle::Normal),
    );
    check_discrete_lerp(
        |v| TextStyle { font_synthesis: v, ..empty() },
        |s| s.font_synthesis,
        Some(FontSynthesis::Style),
        Some(FontSynthesis::Weight),
    );
    check_discrete_lerp(
        |v: Option<compose_ui::SharedString>| TextStyle { font_feature_settings: v, ..empty() },
        |s| s.font_feature_settings.clone(),
        Some("\"kern\" 0".into()),
        Some("\"kern\" 1".into()),
    );
    check_discrete_lerp(
        |v| TextStyle { locale_list: v, ..empty() },
        |s| s.locale_list.clone(),
        Some(LocaleList::from_language_tags("en-US")),
        Some(LocaleList::from_language_tags("ja-JP")),
    );
    check_discrete_lerp(
        |v| TextStyle { text_decoration: v, ..empty() },
        |s| s.text_decoration,
        Some(TextDecoration::LINE_THROUGH),
        Some(TextDecoration::UNDERLINE),
    );
    check_discrete_lerp(
        |v| TextStyle { draw_style: v, ..empty() },
        |s| s.draw_style,
        Some(DrawStyle::Fill),
        Some(DrawStyle::Stroke(Stroke::new(8.0))),
    );
    let align = |v| TextStyle { text_align: v, ..empty() };
    check_discrete_lerp(align, |s| s.text_align, None, Some(TextAlign::Right));
    check_discrete_lerp(align, |s| s.text_align, Some(TextAlign::Left), Some(TextAlign::Right));
    let direction = |v| TextStyle { text_direction: v, ..empty() };
    check_discrete_lerp(direction, |s| s.text_direction, None, Some(TextDirection::Rtl));
    check_discrete_lerp(
        direction,
        |s| s.text_direction,
        Some(TextDirection::Ltr),
        Some(TextDirection::Rtl),
    );
    let line_break = |v| TextStyle { line_break: v, ..empty() };
    check_discrete_lerp(line_break, |s| s.line_break, Some(LineBreak::HEADING), None);
    check_discrete_lerp(line_break, |s| s.line_break, None, Some(LineBreak::HEADING));
    let motion = |v| TextStyle { text_motion: v, ..empty() };
    check_discrete_lerp(motion, |s| s.text_motion, Some(TextMotion::Animated), None);
    check_discrete_lerp(motion, |s| s.text_motion, None, Some(TextMotion::Animated));
    let line_height_style = |v| TextStyle { line_height_style: v, ..empty() };
    let get = |s: &TextStyle| s.line_height_style;
    check_discrete_lerp(line_height_style, get, None, None);
    check_discrete_lerp(line_height_style, get, Some(LineHeightStyle::DEFAULT), None);
    check_discrete_lerp(line_height_style, get, None, Some(LineHeightStyle::DEFAULT));
    assert_eq!(TextStyle::lerp(&empty(), &empty(), 0.5).platform_style, None);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp color with a and b are specified
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp fontSize with a and b are not Null
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp fontWeight with a and b are not Null
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp baselineShift with a and b are not Null
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp textGeometricTransform with a and b are not Null
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp background with a and b are Null and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp background with a is Null and b is not Null
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp background with a is Not Null and b is Null
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp background with a and b are not Null and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp background with a and b are not Null and t is larger than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp textIndent with a null, b not null and t is smaller than half returns null
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp textIndent with a and b are not Null
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp lineHeight with a and b are specified
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp lineHeight with a and b are unspecified
#[test]
fn lerping_numeric_fields_interpolates_them() {
    let lerp = |a: TextStyle, b: TextStyle, t| TextStyle::lerp(&a, &b, t);
    let lerped = lerp(
        TextStyle { color: Color::RED, ..empty() },
        TextStyle { color: Color::GREEN, ..empty() },
        0.3,
    );
    assert_eq!(lerped.foreground().color(), Color::lerp(Color::RED, Color::GREEN, 0.3));
    let lerped = lerp(
        TextStyle { font_size: 8.sp(), ..empty() },
        TextStyle { font_size: 16.sp(), ..empty() },
        0.8,
    );
    assert_eq!(lerped.font_size, 14.4.sp());
    let weights = (FontWeight::W200, FontWeight::W500);
    let lerped = lerp(
        TextStyle { font_weight: Some(weights.0), ..empty() },
        TextStyle { font_weight: Some(weights.1), ..empty() },
        0.8,
    );
    assert_eq!(lerped.font_weight, Some(FontWeight::lerp(weights.0, weights.1, 0.8)));
    let shifts = (BaselineShift(1.0), BaselineShift(2.0));
    let lerped = lerp(
        TextStyle { baseline_shift: Some(shifts.0), ..empty() },
        TextStyle { baseline_shift: Some(shifts.1), ..empty() },
        0.3,
    );
    assert_eq!(lerped.baseline_shift, Some(BaselineShift::lerp(shifts.0, shifts.1, 0.3)));
    let transforms = (
        TextGeometricTransform { scale_x: 1.5, skew_x: 0.1 },
        TextGeometricTransform { scale_x: 1.0, skew_x: 0.3 },
    );
    let lerped = lerp(
        TextStyle { text_geometric_transform: Some(transforms.0), ..empty() },
        TextStyle { text_geometric_transform: Some(transforms.1), ..empty() },
        0.3,
    );
    assert_eq!(
        lerped.text_geometric_transform,
        Some(TextGeometricTransform::lerp(transforms.0, transforms.1, 0.3))
    );
    let background = |background| TextStyle { background, ..empty() };
    for (a, b, t) in [
        (Color::UNSPECIFIED, Color::UNSPECIFIED, 0.1),
        (Color::UNSPECIFIED, Color::RED, 0.1),
        (Color::RED, Color::UNSPECIFIED, 0.1),
        (Color::RED, Color::GREEN, 0.2),
        (Color::RED, Color::GREEN, 0.8),
    ] {
        assert_eq!(lerp(background(a), background(b), t).background, Color::lerp(a, b, t));
    }
    let with_indent = |text_indent| TextStyle { text_indent, ..empty() };
    let lerped = lerp(with_indent(None), with_indent(Some(indent(20.sp()))), 0.4);
    assert_eq!(lerped.text_indent, Some(TextIndent::lerp(TextIndent::NONE, indent(20.sp()), 0.4)));
    let lerped = lerp(with_indent(Some(indent(10.sp()))), with_indent(Some(indent(20.sp()))), 0.6);
    assert_eq!(lerped.text_indent, Some(TextIndent::lerp(indent(10.sp()), indent(20.sp()), 0.6)));
    let with_height = |line_height| TextStyle { line_height, ..empty() };
    let lerped = lerp(with_height(10.sp()), with_height(20.sp()), 0.4);
    assert_eq!(lerped.line_height, TextUnit::lerp(10.sp(), 20.sp(), 0.4));
    let lerped = lerp(with_height(TextUnit::UNSPECIFIED), with_height(TextUnit::UNSPECIFIED), 0.4);
    assert_eq!(lerped.line_height, TextUnit::UNSPECIFIED);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp brush with a specified, b specified and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp brush with a specified, b specified and t is larger than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, lerp brush with a specified, b not specified and t is larger than half
#[test]
fn lerping_a_brush_and_a_color_switches_half_way() {
    let brushed = TextStyle { brush: Some(gradient()), ..empty() };
    let red = TextStyle { color: Color::RED, ..empty() };
    let lerped = TextStyle::lerp(&brushed, &red, 0.4);
    assert_eq!((lerped.brush, lerped.color), (Some(gradient()), Color::UNSPECIFIED));
    let lerped = TextStyle::lerp(&brushed, &red, 0.6);
    assert_eq!((lerped.brush, lerped.color), (None, Color::RED));
    let lerped = TextStyle::lerp(&brushed, &empty(), 0.6);
    assert_eq!((lerped.brush, lerped.color), (None, Color::UNSPECIFIED));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, toSpanStyle return attributes with correct values
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, toSpanStyle return attributes with correct values for brush
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, toParagraphStyle return attributes with correct values
#[test]
fn a_text_style_splits_into_span_and_paragraph_styles() {
    let shadow = Shadow { color: Color::GREEN, offset: Offset::new(2.0, 4.0), ..Shadow::NONE };
    let span = SpanStyle {
        font_size: 56.sp(),
        font_weight: Some(FontWeight::BOLD),
        font_style: Some(FontStyle::Italic),
        font_synthesis: Some(FontSynthesis::All),
        font_family: Some(FontFamily::SansSerif),
        font_feature_settings: Some("font feature settings".into()),
        letter_spacing: 0.2.sp(),
        baseline_shift: Some(BaselineShift::SUBSCRIPT),
        text_geometric_transform: Some(TextGeometricTransform { scale_x: 0.5, skew_x: 0.6 }),
        locale_list: Some(LocaleList::from_language_tags("tr-TR")),
        background: Color::YELLOW,
        text_decoration: Some(TextDecoration::UNDERLINE),
        shadow: Some(shadow),
        ..SpanStyle::DEFAULT
    };
    for paint in [
        SpanStyle { color: Color::RED, ..span.clone() },
        SpanStyle {
            brush: Some(gradient()),
            draw_style: Some(DrawStyle::Stroke(Stroke::new(8.0))),
            ..span.clone()
        },
    ] {
        let style = TextStyle::from_styles(paint.clone(), ParagraphStyle::DEFAULT);
        assert_eq!(style.to_span_style(), paint);
    }
    let paragraph = ParagraphStyle {
        text_align: Some(TextAlign::Justify),
        text_direction: Some(TextDirection::Rtl),
        line_height: 100.sp(),
        text_indent: Some(TextIndent { first_line: 20.sp(), rest_line: 40.sp() }),
        line_height_style: Some(LineHeightStyle::new(
            LineHeightAlignment::CENTER,
            LineHeightTrim::None,
        )),
        hyphens: Some(Hyphens::Auto),
        line_break: Some(LineBreak {
            strategy: LineBreakStrategy::Balanced,
            strictness: LineBreakStrictness::Strict,
            word_break: LineBreakWordBreak::Phrase,
        }),
        text_motion: Some(TextMotion::Animated),
        ..ParagraphStyle::DEFAULT
    };
    assert_eq!(
        TextStyle::from_styles(SpanStyle::DEFAULT, paragraph).to_paragraph_style(),
        paragraph
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, negative lineHeight throws IllegalStateException
#[test]
#[should_panic(expected = "lineHeight can't be negative")]
fn a_negative_line_height_is_rejected() {
    TextStyle { line_height: (-1).sp(), ..empty() }.to_paragraph_style().validate();
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, equals return false for different line height behavior
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, equals return true for same line height behavior
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, hashCode is same for same line height behavior
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, hashCode is different for different line height behavior
#[test]
fn styles_compare_and_hash_by_their_fields() {
    let with = |line_height_style| TextStyle { line_height_style, ..empty() };
    assert_ne!(with(None), with(Some(LineHeightStyle::DEFAULT)));
    assert_eq!(with(Some(LineHeightStyle::DEFAULT)), with(Some(LineHeightStyle::DEFAULT)));
    assert_eq!(
        hash_of(&with(Some(LineHeightStyle::DEFAULT))),
        hash_of(&with(Some(LineHeightStyle::DEFAULT)))
    );
    assert_ne!(
        hash_of(&with(Some(LineHeightStyle::new(
            LineHeightAlignment::BOTTOM,
            LineHeightTrim::None
        )))),
        hash_of(&with(Some(LineHeightStyle::new(
            LineHeightAlignment::CENTER,
            LineHeightTrim::Both
        ))))
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, resolveTextDirection_unspecified
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, resolveTextDirection_Content
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, resolveTextDirection_ContentOrLtr
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, resolveTextDirection_ContentOrRtl
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, resolveTextDirection_Ltr
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, resolveTextDirection_Rtl
#[test]
fn a_text_direction_resolves_against_the_layout_direction() {
    use LayoutDirection::{Ltr, Rtl};
    let cases = [
        (None, TextDirection::Ltr, TextDirection::Rtl),
        (Some(TextDirection::Content), TextDirection::ContentOrLtr, TextDirection::ContentOrRtl),
        (
            Some(TextDirection::ContentOrLtr),
            TextDirection::ContentOrLtr,
            TextDirection::ContentOrLtr,
        ),
        (
            Some(TextDirection::ContentOrRtl),
            TextDirection::ContentOrRtl,
            TextDirection::ContentOrRtl,
        ),
        (Some(TextDirection::Ltr), TextDirection::Ltr, TextDirection::Ltr),
        (Some(TextDirection::Rtl), TextDirection::Rtl, TextDirection::Rtl),
    ];
    for (direction, ltr, rtl) in cases {
        assert_eq!(TextDirection::resolve(direction, Ltr), ltr);
        assert_eq!(TextDirection::resolve(direction, Rtl), rtl);
    }
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/TextStyleTest.kt, textStyle_allParamsMerge_nonDefaultWins
#[test]
fn every_field_set_wins_a_merge_with_the_default() {
    // A value differing from the default for each field (Compose finds the fields by reflection).
    let others: Vec<(&str, TextStyle)> = vec![
        ("color", TextStyle { color: Color::MAGENTA, ..empty() }),
        ("brush", TextStyle { brush: Some(gradient()), alpha: 0.7, ..empty() }),
        (
            "alpha",
            TextStyle { brush: Some(Brush::linear_gradient([Color::RED])), alpha: 1.0, ..empty() },
        ),
        ("font_size", TextStyle { font_size: 4.em(), ..empty() }),
        ("font_weight", TextStyle { font_weight: Some(FontWeight::new(712)), ..empty() }),
        ("font_style", TextStyle { font_style: Some(FontStyle::Italic), ..empty() }),
        ("font_synthesis", TextStyle { font_synthesis: Some(FontSynthesis::Weight), ..empty() }),
        ("font_family", TextStyle { font_family: Some(FontFamily::Cursive), ..empty() }),
        (
            "font_feature_settings",
            TextStyle { font_feature_settings: Some(" more text".into()), ..empty() },
        ),
        ("letter_spacing", TextStyle { letter_spacing: 4.em(), ..empty() }),
        (
            "baseline_shift",
            TextStyle { baseline_shift: Some(BaselineShift::SUPERSCRIPT), ..empty() },
        ),
        (
            "text_geometric_transform",
            TextStyle {
                text_geometric_transform: Some(TextGeometricTransform {
                    scale_x: 0.5,
                    skew_x: 0.5,
                }),
                ..empty()
            },
        ),
        (
            "locale_list",
            TextStyle { locale_list: Some(LocaleList::from_language_tags("fr")), ..empty() },
        ),
        ("background", TextStyle { background: Color::MAGENTA, ..empty() }),
        (
            "text_decoration",
            TextStyle { text_decoration: Some(TextDecoration::UNDERLINE), ..empty() },
        ),
        (
            "shadow",
            TextStyle { shadow: Some(Shadow { color: Color::RED, ..Shadow::NONE }), ..empty() },
        ),
        (
            "draw_style",
            TextStyle { draw_style: Some(DrawStyle::Stroke(Stroke::new(1.0))), ..empty() },
        ),
        ("text_align", TextStyle { text_align: Some(TextAlign::Justify), ..empty() }),
        ("text_direction", TextStyle { text_direction: Some(TextDirection::Rtl), ..empty() }),
        ("line_height", TextStyle { line_height: 4.em(), ..empty() }),
        ("text_indent", TextStyle { text_indent: Some(indent(10.sp())), ..empty() }),
        (
            "platform_style",
            TextStyle {
                platform_style: Some(PlatformTextStyle::with_emoji_support_match(
                    EmojiSupportMatch::None,
                )),
                ..empty()
            },
        ),
        (
            "line_height_style",
            TextStyle {
                line_height_style: Some(LineHeightStyle::new(
                    LineHeightAlignment::CENTER,
                    LineHeightTrim::None,
                )),
                ..empty()
            },
        ),
        ("line_break", TextStyle { line_break: Some(LineBreak::HEADING), ..empty() }),
        ("hyphens", TextStyle { hyphens: Some(Hyphens::Auto), ..empty() }),
        ("text_motion", TextStyle { text_motion: Some(TextMotion::Animated), ..empty() }),
    ];
    let subject = TextStyle::DEFAULT;
    for (field, other) in others {
        assert_ne!(subject.merge(&other), subject, "subject.merge(other) on {field}");
        assert_ne!(other.merge(&subject), subject, "other.merge(subject) on {field}");
    }
}
