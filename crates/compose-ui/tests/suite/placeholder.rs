//! Compose's `Placeholder`, and a measurer's results around them.

use std::rc::Rc;

use compose_ui::platform::{FixedFontMetrics, FixedTextMeasurer, SharedTextMeasurer};
use compose_ui::{
    AnnotatedRange, Density, LayoutDirection, LocaleList, Placeholder, PlaceholderVerticalAlign,
    Placeholders, TextMeasureOptions, TextMeasurer, TextUnit, UnitExt, create_font_family_resolver,
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
    Rc::new([AnnotatedRange::new(Placeholder::new(1.0.em(), 1.0.em(), ALIGN), start, end)])
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
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/CacheTextLayoutInputTest.kt, placeholders_should_differ
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
