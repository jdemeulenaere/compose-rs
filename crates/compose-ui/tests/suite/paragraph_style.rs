//! `ParagraphStyle`, adapted from Compose's `ParagraphStyleTest`.

use std::hash::{Hash, Hasher};

use compose_ui::{
    Hyphens, LineBreak, LineHeightAlignment, LineHeightStyle, LineHeightTrim, ParagraphStyle,
    TextAlign, TextDirection, TextIndent, TextUnit, UnitExt,
};

fn empty() -> ParagraphStyle {
    ParagraphStyle::DEFAULT
}

fn hash_of(style: &ParagraphStyle) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    style.hash(&mut hasher);
    hasher.finish()
}

fn indent(first_line: TextUnit) -> TextIndent {
    TextIndent { first_line, ..TextIndent::NONE }
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, negative lineHeight throws IllegalStateException
#[test]
#[should_panic(expected = "lineHeight can't be negative")]
fn a_negative_line_height_is_rejected() {
    ParagraphStyle { line_height: (-1).sp(), ..empty() }.validate();
}

/// Merges styles made by `with`: `other`'s field when set, else this one's, else unset.
fn check_merge<T: Copy + PartialEq + std::fmt::Debug>(
    with: impl Fn(Option<T>) -> ParagraphStyle,
    get: impl Fn(&ParagraphStyle) -> Option<T>,
    this: T,
    other: T,
) {
    assert_eq!(get(&with(Some(this)).merge(&with(Some(other)))), Some(other), "other's wins");
    assert_eq!(get(&with(Some(this)).merge(&with(None))), Some(this), "unset keeps this");
    assert_eq!(get(&with(None).merge(&with(Some(other)))), Some(other), "unset takes other's");
    assert_eq!(get(&with(None).merge(&with(None))), None, "both unset stay unset");
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, merge textAlign uses other's textAlign
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, merge textAlign other unspecified, return original
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, merge textAlign both unspecified returns unspecified
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, merge textDirection uses other's textDirection
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, merge textDirection other null, returns original
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, merge textDirection both unspecified returns unspecified
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, merge non-null hyphens uses other's hyphens
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, merge hyphens other unspecified, returns original
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, merge unspecified hyphens other non-null, returns other's hyphens
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, merge hyphens both unspecified returns unspecified
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, merge lineHeight uses other's lineHeight
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, merge lineHeight other unspecified, return original
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, merge lineHeight both unspecified returns unspecified
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, merge textIndent uses other's textIndent
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, merge textIndent other null, return original
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, merge textIndent both null returns null
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, merge unspecified with non-null lineBreak uses other's lineBreak
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, merge non-null with unspecified lineBreak returns original's lineBreak
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, merge unspecified with unspecified lineBreak returns unspecified
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, merge non-null with non-null lineBreak returns other's lineBreak
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, merge null platformStyles
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, merge with null lineHeightStyle uses other's lineHeightStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, merge with non-null lineHeightStyle, returns original
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, merge with both null lineHeightStyle returns null
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, merge with both non-null lineHeightStyle returns other's lineHeightStyle
#[test]
fn merging_takes_each_field_from_other_when_set() {
    check_merge(
        |value| ParagraphStyle { text_align: value, ..empty() },
        |style| style.text_align,
        TextAlign::Justify,
        TextAlign::Right,
    );
    check_merge(
        |value| ParagraphStyle { text_direction: value, ..empty() },
        |style| style.text_direction,
        TextDirection::Rtl,
        TextDirection::Ltr,
    );
    check_merge(
        |value| ParagraphStyle { hyphens: value, ..empty() },
        |style| style.hyphens,
        Hyphens::Auto,
        Hyphens::None,
    );
    check_merge(
        |value| ParagraphStyle { line_height: value.unwrap_or(TextUnit::UNSPECIFIED), ..empty() },
        |style| Some(style.line_height).filter(|height| height.is_specified()),
        12.sp(),
        20.sp(),
    );
    check_merge(
        |value| ParagraphStyle { text_indent: value, ..empty() },
        |style| style.text_indent,
        indent(12.sp()),
        indent(20.sp()),
    );
    check_merge(
        |value| ParagraphStyle { line_break: value, ..empty() },
        |style| style.line_break,
        LineBreak::PARAGRAPH,
        LineBreak::HEADING,
    );
    check_merge(
        |value| ParagraphStyle { line_height_style: value, ..empty() },
        |style| style.line_height_style,
        LineHeightStyle::new(LineHeightAlignment::CENTER, LineHeightTrim::None),
        LineHeightStyle::new(LineHeightAlignment::BOTTOM, LineHeightTrim::Both),
    );
    assert_eq!(empty().merge(&empty()).platform_style, None);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, plus operator merges
#[test]
fn plus_merges() {
    let style = ParagraphStyle {
        text_align: Some(TextAlign::Center),
        text_direction: Some(TextDirection::Rtl),
        ..empty()
    } + ParagraphStyle {
        text_align: Some(TextAlign::Justify),
        line_height: 12.sp(),
        ..empty()
    };
    assert_eq!(
        style,
        ParagraphStyle {
            text_align: Some(TextAlign::Justify),
            text_direction: Some(TextDirection::Rtl),
            line_height: 12.sp(),
            ..empty()
        }
    );
}

/// Lerps styles made by `with` from `start` to `stop`: `start`'s below half way, `stop`'s from
/// there (half way included).
fn check_discrete_lerp<T: Copy + PartialEq + std::fmt::Debug>(
    with: impl Fn(Option<T>) -> ParagraphStyle,
    get: impl Fn(&ParagraphStyle) -> Option<T>,
    start: Option<T>,
    stop: Option<T>,
) {
    let lerped = |t| get(&ParagraphStyle::lerp(&with(start), &with(stop), t));
    assert_eq!(lerped(0.4), start);
    assert_eq!(lerped(0.5), stop);
    assert_eq!(lerped(0.6), stop);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, lerp textAlign with a unspecified, b not null and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, lerp textAlign with a and b are not Null and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, lerp textAlign with a and b are not Null and t is larger than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, lerp textDirection with a unspecified, b not null and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, lerp textDirection with a and b are not Null and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, lerp textDirection with a and b are not Null and t is larger than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, lerp hyphens with a unspecified, b not null and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, lerp hyphens with a unspecified, b not null and t is equal to half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, lerp hyphens with a and b are not null and t is smaller than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, lerp hyphens with a and b are not Null and t is larger than half
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, lerp with null lineHeightStyles has null lineHeightStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, lerp with non-null start, null end, closer to start has non-null lineHeightStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, lerp with non-null start, null end, closer to end has null lineHeightStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, lerp with null start, non-null end, closer to start has null lineHeightStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, lerp with null start, non-null end, closer to end has non-null lineHeightStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, lerp with non-null start, unspecified end, closer to start has non-null lineBreak
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, lerp with non-null start, unspecified end, closer to end has unspecified lineBreak
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, lerp with unspecified start, non-null end, closer to start has unspecified lineBreak
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, lerp with unspecified start, non-null end, closer to end has non-null lineBreak
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, lerp with null platformStyles has null platformStyle
#[test]
fn lerping_discrete_fields_switches_half_way() {
    let align = |value| ParagraphStyle { text_align: value, ..empty() };
    check_discrete_lerp(align, |style| style.text_align, None, Some(TextAlign::Right));
    check_discrete_lerp(
        align,
        |style| style.text_align,
        Some(TextAlign::Left),
        Some(TextAlign::Right),
    );
    let direction = |value| ParagraphStyle { text_direction: value, ..empty() };
    check_discrete_lerp(direction, |style| style.text_direction, None, Some(TextDirection::Rtl));
    check_discrete_lerp(
        direction,
        |style| style.text_direction,
        Some(TextDirection::Ltr),
        Some(TextDirection::Rtl),
    );
    let hyphens = |value| ParagraphStyle { hyphens: value, ..empty() };
    check_discrete_lerp(hyphens, |style| style.hyphens, None, Some(Hyphens::Auto));
    check_discrete_lerp(hyphens, |style| style.hyphens, Some(Hyphens::Auto), Some(Hyphens::None));
    let line_height_style = |value| ParagraphStyle { line_height_style: value, ..empty() };
    let get = |style: &ParagraphStyle| style.line_height_style;
    check_discrete_lerp(line_height_style, get, None, None);
    check_discrete_lerp(line_height_style, get, Some(LineHeightStyle::DEFAULT), None);
    check_discrete_lerp(line_height_style, get, None, Some(LineHeightStyle::DEFAULT));
    let line_break = |value| ParagraphStyle { line_break: value, ..empty() };
    check_discrete_lerp(line_break, |style| style.line_break, Some(LineBreak::HEADING), None);
    check_discrete_lerp(line_break, |style| style.line_break, None, Some(LineBreak::HEADING));
    assert_eq!(ParagraphStyle::lerp(&empty(), &empty(), 0.5).platform_style, None);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, lerp textIndent with a null, b not null and t is smaller than half returns null
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, lerp textIndent with a and b are not Null
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, lerp lineHeight with a and b are specified
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, lerp lineHeight with a and b are unspecified
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, lerp lineHeight with either a or b is unspecified
#[test]
fn lerping_indents_and_line_heights_interpolates_them() {
    let with_indent = |value| ParagraphStyle { text_indent: value, ..empty() };
    let lerped = ParagraphStyle::lerp(&with_indent(None), &with_indent(Some(indent(20.sp()))), 0.4);
    assert_eq!(lerped.text_indent, Some(TextIndent::lerp(TextIndent::NONE, indent(20.sp()), 0.4)));
    let lerped = ParagraphStyle::lerp(
        &with_indent(Some(indent(10.sp()))),
        &with_indent(Some(indent(20.sp()))),
        0.6,
    );
    assert_eq!(lerped.text_indent, Some(TextIndent::lerp(indent(10.sp()), indent(20.sp()), 0.6)));
    let with_height = |line_height| ParagraphStyle { line_height, ..empty() };
    let lerped = ParagraphStyle::lerp(&with_height(10.sp()), &with_height(20.sp()), 0.4);
    assert_eq!(lerped.line_height, TextUnit::lerp(10.sp(), 20.sp(), 0.4));
    let unspecified = with_height(TextUnit::UNSPECIFIED);
    assert_eq!(
        ParagraphStyle::lerp(&unspecified, &unspecified, 0.4).line_height,
        TextUnit::UNSPECIFIED
    );
    let tall = with_height(22.sp());
    assert_eq!(ParagraphStyle::lerp(&unspecified, &tall, 0.4).line_height, TextUnit::UNSPECIFIED);
    assert_eq!(ParagraphStyle::lerp(&unspecified, &tall, 0.8).line_height, 22.sp());
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, equals return false for different line height behavior
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, equals return true for same line height behavior
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, hashCode is same for same line height behavior
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, hashCode is different for different line height behavior
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, equals return false for different and non-null hyphens behavior
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, equals return false for unspecified and non-null hyphens
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, equals return true for same and non-null hyphens behavior
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, equals return true for both unspecified hyphens
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, hashCode is same for same hyphens behavior
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, hashCode is different for different hyphens behavior
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, hashCode is same for same lineBreak
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, hashCode is different for different lineBreak
#[test]
fn styles_compare_and_hash_by_their_fields() {
    let with_style = |value| ParagraphStyle { line_height_style: value, ..empty() };
    assert_ne!(with_style(None), with_style(Some(LineHeightStyle::DEFAULT)));
    assert_eq!(
        with_style(Some(LineHeightStyle::DEFAULT)),
        with_style(Some(LineHeightStyle::DEFAULT))
    );
    assert_eq!(
        hash_of(&with_style(Some(LineHeightStyle::DEFAULT))),
        hash_of(&with_style(Some(LineHeightStyle::DEFAULT)))
    );
    assert_ne!(
        hash_of(&with_style(Some(LineHeightStyle::new(
            LineHeightAlignment::BOTTOM,
            LineHeightTrim::None
        )))),
        hash_of(&with_style(Some(LineHeightStyle::new(
            LineHeightAlignment::CENTER,
            LineHeightTrim::Both
        ))))
    );
    let with_hyphens = |value| ParagraphStyle { hyphens: value, ..empty() };
    assert_ne!(with_hyphens(Some(Hyphens::None)), with_hyphens(Some(Hyphens::Auto)));
    assert_ne!(with_hyphens(None), with_hyphens(Some(Hyphens::Auto)));
    assert_eq!(with_hyphens(Some(Hyphens::Auto)), with_hyphens(Some(Hyphens::Auto)));
    assert_eq!(with_hyphens(None), with_hyphens(None));
    assert_eq!(
        hash_of(&with_hyphens(Some(Hyphens::Auto))),
        hash_of(&with_hyphens(Some(Hyphens::Auto)))
    );
    assert_ne!(
        hash_of(&with_hyphens(Some(Hyphens::None))),
        hash_of(&with_hyphens(Some(Hyphens::Auto)))
    );
    let with_break = |value| ParagraphStyle { line_break: Some(value), ..empty() };
    assert_eq!(
        hash_of(&with_break(LineBreak::PARAGRAPH)),
        hash_of(&with_break(LineBreak::PARAGRAPH))
    );
    assert_ne!(
        hash_of(&with_break(LineBreak::PARAGRAPH)),
        hash_of(&with_break(LineBreak::HEADING))
    );
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, copy with lineHeightStyle returns new lineHeightStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, copy without lineHeightStyle uses existing lineHeightStyle
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, copy with hyphens returns new hyphens
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, copy without hyphens uses existing hyphens
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, copy with lineBreak returns new lineBreak
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, copy without lineBreak uses existing lineBreak
// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/ParagraphStyleTest.kt, constructor without lineHeightStyle sets lineHeightStyle to null
#[test]
fn struct_update_copies_with_one_field_replaced() {
    let bottom = LineHeightStyle::new(LineHeightAlignment::BOTTOM, LineHeightTrim::None);
    let centered = LineHeightStyle::new(LineHeightAlignment::CENTER, LineHeightTrim::Both);
    let style = ParagraphStyle {
        line_height_style: Some(bottom),
        hyphens: Some(Hyphens::None),
        line_break: Some(LineBreak::PARAGRAPH),
        ..empty()
    };
    assert_eq!(
        ParagraphStyle { line_height_style: Some(centered), ..style }.line_height_style,
        Some(centered)
    );
    assert_eq!(
        ParagraphStyle { hyphens: Some(Hyphens::Auto), ..style }.hyphens,
        Some(Hyphens::Auto)
    );
    assert_eq!(
        ParagraphStyle { line_break: Some(LineBreak::HEADING), ..style }.line_break,
        Some(LineBreak::HEADING)
    );
    let copy = style;
    assert_eq!(
        (copy.line_height_style, copy.hyphens, copy.line_break),
        (Some(bottom), Some(Hyphens::None), Some(LineBreak::PARAGRAPH))
    );
    assert_eq!(
        ParagraphStyle { text_align: Some(TextAlign::Start), ..empty() }.line_height_style,
        None
    );
}
