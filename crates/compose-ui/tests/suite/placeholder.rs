//! Compose's `Placeholder`, and a measurer's results around them.

use std::rc::Rc;

use compose_ui::platform::{FixedFontMetrics, FixedTextMeasurer, SharedTextMeasurer};
use compose_ui::{
    AnnotatedRange, Density, LayoutDirection, LocaleList, Paragraph, Placeholder,
    PlaceholderVerticalAlign, Placeholders, Rect, TextDirection, TextMeasureOptions, TextMeasurer,
    TextOverflow, TextStyle, TextUnit, UnitExt, create_font_family_resolver,
};

const ALIGN: PlaceholderVerticalAlign = PlaceholderVerticalAlign::AboveBaseline;

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/PlaceholderTest.kt, width_isUnspecified
#[test]
#[should_panic(expected = "width cannot be TextUnit.Unspecified")]
fn a_placeholder_needs_a_width() {
    Placeholder::new(TextUnit::UNSPECIFIED, 1.0.em(), ALIGN);
}

// Compose: compose/ui/ui-text/src/androidHostTest/kotlin/androidx/compose/ui/text/PlaceholderTest.kt, height_isUnspecified
#[test]
#[should_panic(expected = "height cannot be TextUnit.Unspecified")]
fn a_placeholder_needs_a_height() {
    Placeholder::new(1.0.em(), TextUnit::UNSPECIFIED, ALIGN);
}

fn measurer(cache: usize) -> TextMeasurer {
    let platform = SharedTextMeasurer::new(Box::new(FixedTextMeasurer::with_metrics(
        FixedFontMetrics::SAMPLE_FONT,
    )));
    TextMeasurer::new(
        create_font_family_resolver(&platform),
        LocaleList::empty(),
        Density::new(1.0),
        LayoutDirection::Ltr,
        cache,
    )
}

fn one_at(start: usize, end: usize) -> Placeholders {
    Some(Rc::new([AnnotatedRange::new(Placeholder::new(1.0.em(), 1.0.em(), ALIGN), start, end)]))
}

fn options_with(placeholders: Placeholders) -> TextMeasureOptions {
    TextMeasureOptions { placeholders, ..TextMeasureOptions::default() }
}

#[test]
fn a_measured_text_says_where_its_placeholders_went() {
    let result = measurer(0).measure("abc", options_with(one_at(1, 2)));
    let rects = result.placeholder_rects();
    assert_eq!(rects.len(), 1);
    assert!(rects[0].is_some());
    assert_eq!(result.layout_input.placeholder_ranges().len(), 1);
}

#[test]
fn another_placeholder_is_another_layout_to_the_cache() {
    let measurer = measurer(8);
    let first = measurer.measure("abc", options_with(one_at(1, 2)));
    let same = measurer.measure("abc", options_with(one_at(1, 2)));
    let moved = measurer.measure("abc", options_with(one_at(0, 1)));
    let none = measurer.measure("abc", TextMeasureOptions::default());
    assert!(Rc::ptr_eq(&first.multi_paragraph, &same.multi_paragraph));
    assert!(!Rc::ptr_eq(&first.multi_paragraph, &moved.multi_paragraph));
    assert!(!Rc::ptr_eq(&first.multi_paragraph, &none.multi_paragraph));
    assert_eq!(none.placeholder_rects().len(), 0);
}

#[test]
#[should_panic(expected = "isn't in a text of 3 bytes")]
fn a_placeholder_past_the_text_is_refused() {
    measurer(0).measure("abc", options_with(one_at(2, 4)));
}

#[test]
#[should_panic(expected = "at character boundaries")]
fn a_placeholder_inside_a_character_is_refused() {
    measurer(0).measure("\u{5D0}bc", options_with(one_at(1, 2)));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, placeholder_correctPosition_rtlLayout
#[test]
fn a_placeholder_opening_a_line_of_a_right_to_left_paragraph_is_on_its_run_s_left() {
    let before = "Hello ";
    let text = format!("{before}\u{FFFD} World Hello World Hello World");
    let style = TextStyle {
        font_size: 10.0.sp(),
        text_direction: Some(TextDirection::Rtl),
        ..TextStyle::DEFAULT
    };
    let placeholder = Placeholder::new(10.0.sp(), 10.0.sp(), ALIGN);
    let range =
        AnnotatedRange::new(placeholder, before.len(), before.len() + '\u{FFFD}'.len_utf8());
    let result = measurer(0).measure(
        text,
        TextMeasureOptions {
            style,
            placeholders: Some(Rc::new([range])),
            max_lines: 2,
            overflow: TextOverflow::Ellipsis,
            constraints: compose_ui::Constraints::new(0, 60, 0, compose_ui::Constraints::INFINITY),
            ..TextMeasureOptions::default()
        },
    );
    assert_eq!(result.placeholder_rects(), vec![Some(Rect::new(0.0, 10.0, 10.0, 20.0))]);
    let _: &dyn Paragraph = &*result.multi_paragraph;
}
