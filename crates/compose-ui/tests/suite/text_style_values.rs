//! The values text styles hold (font weights, decorations, baseline shifts, line height styles,
//! the paint, the platform parts), adapted from Compose's `FontWeightTest`,
//! `TextDecorationTest`, `BaselineShiftTest`, `LineHeightStyleTest`, `TextForegroundStyleTest`,
//! `PlatformParagraphStyleTest`, `PlatformSpanStyleTest` and `PlatformTextStyleTest`.

use std::hash::{Hash, Hasher};

use compose_ui::{
    BaselineShift, Brush, Color, EmojiSupportMatch, FontWeight, LineHeightAlignment,
    LineHeightMode, LineHeightStyle, LineHeightTrim, ParagraphStyle, PlatformParagraphStyle,
    PlatformSpanStyle, PlatformTextStyle, SpanStyle, TextDecoration, TextForegroundStyle,
    TextStyle,
};

fn hash_of(value: &impl Hash) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontWeightTest.kt, constructor accept 1000
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontWeightTest.kt, constructor accept 1
#[test]
fn a_weight_takes_one_to_a_thousand() {
    assert_eq!(FontWeight::new(1000).weight(), 1000);
    assert_eq!(FontWeight::new(1).weight(), 1);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontWeightTest.kt, constructor does not accept greater than 1000
#[test]
#[should_panic(expected = "Font weight can be in range [1, 1000]")]
fn a_weight_above_a_thousand_panics() {
    FontWeight::new(1001);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontWeightTest.kt, constructor does not accept less than 1
#[test]
#[should_panic(expected = "Font weight can be in range [1, 1000]")]
fn a_weight_of_zero_panics() {
    FontWeight::new(0);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontWeightTest.kt, lerp at start returns start value
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontWeightTest.kt, lerp at start returns font weight 1
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontWeightTest.kt, lerp at end returns end value
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontWeightTest.kt, lerp in the mid-time
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontWeightTest.kt, lerp in the mid-time with odd distance should be rounded to up
#[test]
fn weights_lerp_rounded() {
    assert_eq!(FontWeight::lerp(FontWeight::W200, FontWeight::W400, 0.0), FontWeight::W200);
    assert_eq!(FontWeight::lerp(FontWeight::new(1), FontWeight::W400, 0.0), FontWeight::new(1));
    assert_eq!(FontWeight::lerp(FontWeight::W200, FontWeight::W400, 1.0), FontWeight::W400);
    assert_eq!(FontWeight::lerp(FontWeight::W200, FontWeight::W800, 0.5), FontWeight::W500);
    assert_eq!(FontWeight::lerp(FontWeight::W200, FontWeight::W900, 0.5), FontWeight::new(550));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontWeightTest.kt, values return all weights
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontWeightTest.kt, weight returns collect values
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/font/FontWeightTest.kt, compareTo
#[test]
fn the_named_weights_are_hundreds_in_order() {
    let weights: Vec<u16> = FontWeight::VALUES.iter().map(|weight| weight.weight()).collect();
    assert_eq!(weights, [100, 200, 300, 400, 500, 600, 700, 800, 900]);
    assert_eq!(
        FontWeight::VALUES,
        [
            FontWeight::W100,
            FontWeight::W200,
            FontWeight::W300,
            FontWeight::W400,
            FontWeight::W500,
            FontWeight::W600,
            FontWeight::W700,
            FontWeight::W800,
            FontWeight::W900,
        ]
    );
    use std::cmp::Ordering::{Equal, Greater, Less};
    assert_eq!(FontWeight::W400.cmp(&FontWeight::W400), Equal);
    assert_eq!(FontWeight::W400.cmp(&FontWeight::W300), Greater);
    assert_eq!(FontWeight::W400.cmp(&FontWeight::W500), Less);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/TextDecorationTest.kt, contains with single textDecoration
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/TextDecorationTest.kt, contains with multiple textDecorations
#[test]
fn a_decoration_contains_its_lines_and_none() {
    let none = TextDecoration::NONE;
    assert!(none.contains(TextDecoration::NONE));
    assert!(!none.contains(TextDecoration::LINE_THROUGH));
    let both = TextDecoration::UNDERLINE + TextDecoration::LINE_THROUGH;
    assert!(both.contains(TextDecoration::UNDERLINE));
    assert!(both.contains(TextDecoration::LINE_THROUGH));
    assert!(both.contains(TextDecoration::NONE));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/TextDecorationTest.kt, combine with empty list returns none
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/TextDecorationTest.kt, combine with single element
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/TextDecorationTest.kt, toString with single textDecoration
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/TextDecorationTest.kt, toString with empty combined
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/TextDecorationTest.kt, toString with single combined
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/TextDecorationTest.kt, toString with multiple textDecorations
#[test]
fn decorations_combine_and_print_as_composes() {
    assert_eq!(TextDecoration::combine(&[]), TextDecoration::NONE);
    assert_eq!(TextDecoration::combine(&[TextDecoration::UNDERLINE]), TextDecoration::UNDERLINE);
    assert_eq!(TextDecoration::NONE.to_string(), "TextDecoration.None");
    assert_eq!(TextDecoration::UNDERLINE.to_string(), "TextDecoration.Underline");
    assert_eq!(TextDecoration::LINE_THROUGH.to_string(), "TextDecoration.LineThrough");
    assert_eq!(TextDecoration::combine(&[]).to_string(), "TextDecoration.None");
    assert_eq!(
        TextDecoration::combine(&[TextDecoration::LINE_THROUGH]).to_string(),
        "TextDecoration.LineThrough"
    );
    assert_eq!(
        (TextDecoration::UNDERLINE + TextDecoration::LINE_THROUGH).to_string(),
        "TextDecoration[Underline, LineThrough]"
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/BaselineShiftTest.kt, lerp a with b
#[test]
fn baseline_shifts_lerp() {
    assert_eq!(BaselineShift::lerp(BaselineShift(1.0), BaselineShift(2.0), 0.3).multiplier(), 1.3);
}

fn line_height(
    alignment: LineHeightAlignment,
    trim: LineHeightTrim,
    mode: LineHeightMode,
) -> LineHeightStyle {
    LineHeightStyle { alignment, trim, mode }
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/LineHeightStyleTest.kt, equals_returns_false_for_different_alignment
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/LineHeightStyleTest.kt, equals_returns_false_for_different_trim
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/LineHeightStyleTest.kt, equals_returns_false_for_different_mode
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/LineHeightStyleTest.kt, equals_returns_true_for_same_attributes
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/LineHeightStyleTest.kt, hashCode_is_different_for_different_alignment
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/LineHeightStyleTest.kt, hashCode_is_different_for_different_trim
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/LineHeightStyleTest.kt, hashCode_is_different_for_different_mode
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/LineHeightStyleTest.kt, hashCode_is_same_for_same_attributes
#[test]
fn line_height_styles_compare_and_hash_by_their_fields() {
    use LineHeightAlignment as A;
    use LineHeightMode as M;
    use LineHeightTrim as T;
    let center_none = LineHeightStyle::new(A::CENTER, T::None);
    assert_ne!(center_none, LineHeightStyle::new(A::BOTTOM, T::None));
    assert_ne!(center_none, LineHeightStyle::new(A::CENTER, T::Both));
    assert_ne!(
        line_height(A::CENTER, T::None, M::Fixed),
        line_height(A::CENTER, T::None, M::Minimum)
    );
    assert_eq!(
        line_height(A::CENTER, T::FirstLineTop, M::Tight),
        line_height(A::CENTER, T::FirstLineTop, M::Tight)
    );
    assert_ne!(hash_of(&center_none), hash_of(&LineHeightStyle::new(A::BOTTOM, T::Both)));
    assert_ne!(hash_of(&center_none), hash_of(&LineHeightStyle::new(A::CENTER, T::Both)));
    assert_ne!(
        hash_of(&line_height(A::CENTER, T::None, M::Fixed)),
        hash_of(&line_height(A::CENTER, T::None, M::Minimum))
    );
    assert_eq!(
        hash_of(&line_height(A::CENTER, T::Both, M::Minimum)),
        hash_of(&line_height(A::CENTER, T::Both, M::Minimum))
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/LineHeightStyleTest.kt, isTrimFirstLineTop
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/LineHeightStyleTest.kt, isTrimLastLineBottom
// (Compose's checks of the flags with a stray high bit have no counterpart: trims are an enum)
#[test]
fn trims_say_which_ends_they_trim() {
    use LineHeightTrim as T;
    assert!(T::FirstLineTop.trims_first_line_top() && T::Both.trims_first_line_top());
    assert!(!T::LastLineBottom.trims_first_line_top() && !T::None.trims_first_line_top());
    assert!(T::LastLineBottom.trims_last_line_bottom() && T::Both.trims_last_line_bottom());
    assert!(!T::FirstLineTop.trims_last_line_bottom() && !T::None.trims_last_line_bottom());
}

fn gradient() -> Brush {
    Brush::linear_gradient([Color::RED, Color::BLUE])
}

fn color(color: Color) -> TextForegroundStyle {
    TextForegroundStyle::from_color(color)
}

fn brushed(brush: Brush, alpha: f32) -> TextForegroundStyle {
    TextForegroundStyle::from_brush(Some(&brush), alpha)
}

const TOLERANCE: f32 = 1.0 / 256.0;

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/TextForegroundStyleTest.kt, color is not equal to unspecified
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/TextForegroundStyleTest.kt, brush is not equal to color
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/TextForegroundStyleTest.kt, different colors are not equal
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/TextForegroundStyleTest.kt, same colors should be equal
#[test]
fn paints_compare_by_kind_and_value() {
    assert_ne!(color(Color::RED), TextForegroundStyle::Unspecified);
    assert_ne!(color(Color::RED), brushed(gradient(), 1.0));
    assert_ne!(color(Color::RED), color(Color::BLUE));
    assert_eq!(color(Color::RED), color(Color::RED));
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/TextForegroundStyleTest.kt, unspecified color initiates an Unspecified TextForegroundStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/TextForegroundStyleTest.kt, specified color initiates only color
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/TextForegroundStyleTest.kt, specified color returns its alpha
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/TextForegroundStyleTest.kt, SolidColor is converted to color
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/TextForegroundStyleTest.kt, SolidColor with alpha is modulated
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/TextForegroundStyleTest.kt, ShaderBrush initiates a brush
#[test]
fn a_paint_is_a_color_or_a_gradient() {
    let unspecified = color(Color::UNSPECIFIED);
    assert_eq!(unspecified, TextForegroundStyle::Unspecified);
    assert_eq!((unspecified.color(), unspecified.brush()), (Color::UNSPECIFIED, None));
    let red = color(Color::RED);
    assert_eq!((red.color(), red.brush(), red.alpha()), (Color::RED, None, 1.0));
    let half = color(Color::RED.with_alpha(0.5));
    assert_eq!((half.color(), half.brush()), (Color::RED.with_alpha(0.5), None));
    assert!((half.alpha() - 0.5).abs() < TOLERANCE);
    let solid = brushed(Brush::solid(Color::RED), 1.0);
    assert_eq!((solid.color(), solid.brush()), (Color::RED, None));
    let modulated = brushed(Brush::solid(Color::RED.with_alpha(0.8)), 0.6);
    assert_eq!((modulated.color(), modulated.brush()), (Color::RED.with_alpha(0.48), None));
    let shader = brushed(gradient(), 0.8);
    assert_eq!((shader.color(), shader.brush()), (Color::UNSPECIFIED, Some(&gradient())));
    assert!((shader.alpha() - 0.8).abs() < TOLERANCE);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/TextForegroundStyleTest.kt, merging unspecified with anything returns anything
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/TextForegroundStyleTest.kt, merging brush with color returns brush
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/TextForegroundStyleTest.kt, merging color with brush returns brush
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/TextForegroundStyleTest.kt, merging color with color returns color
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/TextForegroundStyleTest.kt, merging brush with brush returns brush
#[test]
fn merging_paints_prefers_brushes_then_the_other() {
    let unspecified = TextForegroundStyle::Unspecified;
    let merged = unspecified.merge(&color(Color::RED));
    assert_eq!((merged.color(), merged.brush()), (Color::RED, None));
    let merged = unspecified.merge(&brushed(gradient(), 1.0));
    assert_eq!((merged.color(), merged.brush()), (Color::UNSPECIFIED, Some(&gradient())));
    let merged = brushed(gradient(), 1.0).merge(&color(Color::RED));
    assert_eq!((merged.color(), merged.brush()), (Color::UNSPECIFIED, Some(&gradient())));
    let merged = color(Color::RED).merge(&brushed(gradient(), 1.0));
    assert_eq!(merged.brush(), Some(&gradient()));
    let merged = color(Color::BLUE.with_alpha(0.7)).merge(&color(Color::RED.with_alpha(0.5)));
    assert_eq!(merged.color(), Color::RED.with_alpha(0.5));
    assert!((merged.alpha() - 0.5).abs() < TOLERANCE);
    let black_white = Brush::linear_gradient([Color::WHITE, Color::BLACK]);
    let merged = brushed(gradient(), 0.7).merge(&brushed(black_white.clone(), f32::NAN));
    assert_eq!(merged.brush(), Some(&black_white));
    assert!((merged.alpha() - 0.7).abs() < TOLERANCE);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/TextForegroundStyleTest.kt, lerps colors if both ends are not brush
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/TextForegroundStyleTest.kt, lerps discrete if one end is brush and the other end is color
/// A fraction outside 0..1 (an overshooting easing) is clamped, as Compose's
/// `lerp(Color, Color, Float)` clamps it.
#[test]
fn lerping_colors_clamps_the_fraction() {
    assert_eq!(
        Color::lerp(Color::RED, Color::GREEN, 1.5),
        Color::lerp(Color::RED, Color::GREEN, 1.0)
    );
    assert_eq!(
        Color::lerp(Color::RED, Color::GREEN, -0.5),
        Color::lerp(Color::RED, Color::GREEN, 0.0)
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/style/TextForegroundStyleTest.kt, lerps alpha if both ends are brush
#[test]
fn lerping_paints_lerps_colors_and_brush_alphas() {
    let lerped =
        TextForegroundStyle::lerp(&TextForegroundStyle::Unspecified, &color(Color::RED), 0.4);
    assert_eq!(lerped, color(Color::lerp(Color::UNSPECIFIED, Color::RED, 0.4)));
    let (start, stop) = (brushed(gradient(), 1.0), color(Color::RED));
    assert_eq!(TextForegroundStyle::lerp(&start, &stop, 0.4), start);
    let black_white = Brush::linear_gradient([Color::WHITE, Color::BLACK]);
    let lerped = TextForegroundStyle::lerp(
        &brushed(gradient(), 0.4),
        &brushed(black_white.clone(), 0.7),
        0.6,
    );
    assert_eq!(lerped, brushed(black_white, 0.4 * 0.4 + 0.6 * 0.7));
}

fn padding(include_font_padding: bool) -> PlatformParagraphStyle {
    PlatformParagraphStyle { include_font_padding, ..PlatformParagraphStyle::DEFAULT }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/PlatformParagraphStyleTest.kt, merge_returns_the_same_instance_when_other_is_null
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/PlatformParagraphStyleTest.kt, merge_returns_correct_includeFontPadding
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/PlatformParagraphStyleTest.kt, merge_returns_correct_supportEmoji
#[test]
fn platform_paragraph_styles_merge_as_the_other() {
    let style = PlatformParagraphStyle::default();
    assert_eq!(style.merge(None), style);
    assert!(padding(false).merge(Some(padding(true))).include_font_padding);
    let none = PlatformParagraphStyle { emoji_support_match: EmojiSupportMatch::None, ..style };
    assert_eq!(none.merge(Some(style)).emoji_support_match, EmojiSupportMatch::Default);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/PlatformParagraphStyleTest.kt, paragraphStyle_merge_with_null_platformStyle
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/PlatformParagraphStyleTest.kt, paragraphStyle_merge_platformStyles
#[test]
fn paragraph_styles_merge_their_platform_parts() {
    let with = |platform_style| ParagraphStyle { platform_style, ..ParagraphStyle::DEFAULT };
    for padded in [true, false] {
        let style = with(Some(padding(padded)));
        assert_eq!(style.merge(&ParagraphStyle::DEFAULT).platform_style, style.platform_style);
    }
    let padded = with(Some(padding(true)));
    let other = with(Some(PlatformParagraphStyle {
        emoji_support_match: EmojiSupportMatch::None,
        include_font_padding: false,
    }));
    let merged = padded.merge(&other).platform_style.unwrap();
    assert_eq!(
        (merged.include_font_padding, merged.emoji_support_match),
        (false, EmojiSupportMatch::None)
    );
    let merged = other.merge(&padded).platform_style.unwrap();
    assert_eq!(
        (merged.include_font_padding, merged.emoji_support_match),
        (true, EmojiSupportMatch::Default)
    );
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/PlatformParagraphStyleTest.kt, paragraphStyle_lerp_platformStyles_fraction_start
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/PlatformParagraphStyleTest.kt, paragraphStyle_lerp_platformStyles_fraction_end
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/PlatformParagraphStyleTest.kt, paragraphStyle_lerp_with_null_returns_default
#[test]
fn paragraph_styles_lerp_their_platform_parts() {
    let with = |platform_style| ParagraphStyle { platform_style, ..ParagraphStyle::DEFAULT };
    let start = with(Some(PlatformParagraphStyle {
        emoji_support_match: EmojiSupportMatch::None,
        include_font_padding: true,
    }));
    let stop = with(Some(PlatformParagraphStyle {
        emoji_support_match: EmojiSupportMatch::Default,
        include_font_padding: false,
    }));
    let lerped = ParagraphStyle::lerp(&start, &stop, 0.0).platform_style.unwrap();
    assert_eq!(
        (lerped.include_font_padding, lerped.emoji_support_match),
        (true, EmojiSupportMatch::None)
    );
    let lerped = ParagraphStyle::lerp(&start, &stop, 1.0).platform_style.unwrap();
    assert_eq!(
        (lerped.include_font_padding, lerped.emoji_support_match),
        (false, EmojiSupportMatch::Default)
    );
    let flipped = with(Some(padding(!PlatformParagraphStyle::DEFAULT.include_font_padding)));
    let lerped = ParagraphStyle::lerp(&flipped, &with(None), 1.0).platform_style.unwrap();
    assert_eq!(lerped.include_font_padding, PlatformParagraphStyle::DEFAULT.include_font_padding);
    assert_eq!(lerped.emoji_support_match, EmojiSupportMatch::Default);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/PlatformSpanStyleTest.kt, equals_return_true_if_same_instance
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/PlatformSpanStyleTest.kt, equals_return_true_if_equal_empty_instance
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/PlatformSpanStyleTest.kt, merge_returns_the_same_instance_when_other_is_null
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/PlatformSpanStyleTest.kt, merge_returns_the_same_instance_when_other_is_not_null
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/PlatformSpanStyleTest.kt, lerp_returns_the_same_instance
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/PlatformSpanStyleTest.kt, equals_return_true_for_equal_config
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/PlatformSpanStyleTest.kt, copy_copiesPlatformStyle
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/PlatformSpanStyleTest.kt, copy_withPlatformStyle_returnsInstanceWithPassedStyle
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/PlatformSpanStyleTest.kt, equals_return_true_if_platformStyle_is_same
// (equals_return_false_if_different_class has no counterpart: the types differ at compile time)
#[test]
fn platform_span_styles_hold_nothing() {
    let style = PlatformSpanStyle;
    assert_eq!(style, PlatformSpanStyle::DEFAULT);
    assert_eq!(style.merge(None), style);
    assert_eq!(style.merge(Some(PlatformSpanStyle)), style);
    assert_eq!(PlatformSpanStyle::lerp(style, PlatformSpanStyle, 0.5), style);
    let span = SpanStyle { platform_style: Some(style), ..SpanStyle::DEFAULT };
    assert_eq!(span.clone().platform_style, Some(style));
    assert_eq!(
        SpanStyle { platform_style: Some(style), ..SpanStyle::DEFAULT }.platform_style,
        Some(style)
    );
    assert_eq!(span, SpanStyle { platform_style: Some(style), ..SpanStyle::DEFAULT });
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/PlatformTextStyleTest.kt, toParagraphStyle_returnsNullPlatformStyle_ifNull
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/PlatformTextStyleTest.kt, toParagraphStyle_returnsCorrectPlatformTextConfig_if_nonNull
#[test]
fn a_text_styles_paragraph_part_has_its_platform_paragraph_part() {
    assert_eq!(TextStyle::DEFAULT.to_paragraph_style(), ParagraphStyle::DEFAULT);
    let style = TextStyle {
        platform_style: Some(PlatformTextStyle::with_include_font_padding(true)),
        ..TextStyle::DEFAULT
    };
    assert_eq!(
        style.to_paragraph_style(),
        ParagraphStyle { platform_style: Some(padding(true)), ..ParagraphStyle::DEFAULT }
    );
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/PlatformTextStyleTest.kt, merge_platformStyle_null_on_nonNull
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/PlatformTextStyleTest.kt, merge_platformStyle_nonNull_on_null
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/PlatformTextStyleTest.kt, merge_platformStyle_false_on_true
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/PlatformTextStyleTest.kt, merge_platformStyle_handlesEmoji
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/PlatformTextStyleTest.kt, merge_platformStyle_true_on_false_on
#[test]
fn text_styles_merge_their_platform_parts() {
    let with = |platform_style| TextStyle { platform_style, ..TextStyle::DEFAULT };
    let padded = |include| with(Some(PlatformTextStyle::with_include_font_padding(include)));
    let include = |style: TextStyle| {
        style
            .platform_style
            .and_then(|style| style.paragraph_style)
            .map(|style| style.include_font_padding)
    };
    assert_eq!(include(padded(true).merge(&with(None))), Some(true));
    assert_eq!(include(with(None).merge(&padded(true))), Some(true));
    assert_eq!(include(padded(true).merge(&padded(false))), Some(false));
    assert_eq!(include(padded(false).merge(&padded(true))), Some(true));
    let emoji = |matching| with(Some(PlatformTextStyle::with_emoji_support_match(matching)));
    let merged = emoji(EmojiSupportMatch::None).merge(&emoji(EmojiSupportMatch::Default));
    assert_eq!(
        merged
            .platform_style
            .and_then(|style| style.paragraph_style)
            .map(|style| style.emoji_support_match),
        Some(EmojiSupportMatch::Default)
    );
}
