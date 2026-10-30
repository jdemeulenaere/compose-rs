//! Exact expectations on the real stack: Compose's `ParagraphIntegrationTest` with a font every
//! glyph of which is one em wide, on SkParagraph and on the fixed measurer with the same metrics,
//! so the fixed measurer's tests say what the real one does.

use std::rc::Rc;

use compose::test::basic_measure_font;
use compose::ui::platform::{FixedFontMetrics, FixedTextMeasurer, TextLayoutOptions, TextMeasurer};
use compose::ui::{
    Constraints, Density, Offset, Paragraph, Rect, ResolvedTextDirection, TextDirection, TextRange,
    TextStyle, UnitExt,
};
use compose_skia::SkiaTextMeasurer;

const FONT_SIZE: f32 = 50.0;

fn style() -> TextStyle {
    TextStyle {
        font_size: FONT_SIZE.sp(),
        font_family: Some(basic_measure_font().to_font_family()),
        // Compose's `Paragraph` with no direction goes by its first strong character.
        text_direction: Some(TextDirection::Content),
        ..TextStyle::DEFAULT
    }
}

fn laid_out(
    measurer: &mut dyn TextMeasurer,
    text: &str,
    options: TextLayoutOptions,
    width: f32,
) -> Rc<dyn Paragraph> {
    let width = width as i32;
    let constraints = Constraints::new(width, width, 0, Constraints::INFINITY);
    let handle = measurer.layout(text, &style(), options, constraints, &Density::new(1.0));
    let paragraph = measurer.paragraph(handle);
    measurer.release(handle);
    paragraph
}

/// `check` of `text`'s paragraph at `width`, on each stack.
fn on_each_stack(
    text: &str,
    options: TextLayoutOptions,
    width: f32,
    check: impl Fn(&str, &dyn Paragraph),
) {
    let mut fixed = FixedTextMeasurer::with_metrics(FixedFontMetrics::SAMPLE_FONT);
    check("the fixed measurer", &*laid_out(&mut fixed, text, options, width));
    let mut skia = SkiaTextMeasurer::new();
    check("SkParagraph", &*laid_out(&mut skia, text, options, width));
}

fn plain(text: &str, width: f32, check: impl Fn(&str, &dyn Paragraph)) {
    on_each_stack(text, TextLayoutOptions::default(), width, check);
}

/// A baseline: CoreText's metrics reach Skia with a float's error where Android's are whole
/// pixels (40.000153 for 40).
fn assert_baseline(actual: f32, expected: f32, stack: &str) {
    assert!((actual - expected).abs() < 0.001, "{stack}: {actual}, not {expected}");
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, single_line_default_values
#[test]
fn a_single_line_in_the_sample_font_is_one_em_a_character() {
    for text in ["xyz", "\u{5D0}\u{5D1}\u{5D2}"] {
        plain(text, 200.0, |stack, paragraph| {
            let length = text.chars().count() as f32;
            assert_eq!(paragraph.height(), FONT_SIZE, "{stack}: {text}");
            assert_baseline(paragraph.first_baseline(), FONT_SIZE * 0.8, stack);
            assert_baseline(paragraph.last_baseline(), FONT_SIZE * 0.8, stack);
            assert_eq!(paragraph.max_intrinsic_width(), FONT_SIZE * length, "{stack}: {text}");
            assert_eq!(paragraph.min_intrinsic_width(), FONT_SIZE * length, "{stack}: {text}");
        });
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, line_break_default_values
#[test]
fn a_line_in_the_sample_font_breaks_at_its_width() {
    for text in ["abcdef", "\u{5D0}\u{5D1}\u{5D2}\u{5D3}\u{5D4}\u{5D5}"] {
        plain(text, 3.0 * FONT_SIZE, |stack, paragraph| {
            assert_eq!(paragraph.height(), 2.0 * FONT_SIZE, "{stack}: {text}");
            assert_baseline(paragraph.first_baseline(), FONT_SIZE * 0.8, stack);
            assert_baseline(paragraph.last_baseline(), FONT_SIZE * 1.8, stack);
            assert_eq!(paragraph.max_intrinsic_width(), FONT_SIZE * 6.0, "{stack}: {text}");
            assert_eq!(paragraph.min_intrinsic_width(), FONT_SIZE * 6.0, "{stack}: {text}");
        });
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, newline_default_values
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, newline_and_line_break_default_values
#[test]
fn line_feeds_in_the_sample_font_start_lines_without_widening_any() {
    for text in ["abc\ndef", "\u{5D0}\u{5D1}\u{5D2}\n\u{5D3}\u{5D4}\u{5D5}"] {
        plain(text, 3.0 * FONT_SIZE, |stack, paragraph| {
            assert_eq!(paragraph.height(), 2.0 * FONT_SIZE, "{stack}: {text}");
            assert_baseline(paragraph.last_baseline(), FONT_SIZE * 1.8, stack);
            assert_eq!(paragraph.max_intrinsic_width(), FONT_SIZE * 3.0, "{stack}: {text}");
            assert_eq!(paragraph.min_intrinsic_width(), FONT_SIZE * 3.0, "{stack}: {text}");
        });
        plain(text, 2.0 * FONT_SIZE, |stack, paragraph| {
            assert_eq!(paragraph.height(), 4.0 * FONT_SIZE, "{stack}: {text}");
            assert_baseline(paragraph.last_baseline(), FONT_SIZE * 3.8, stack);
        });
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getOffsetForPosition_ltr
#[test]
fn a_position_in_the_sample_font_is_the_character_under_it() {
    plain("abc", 3.0 * FONT_SIZE, |stack, paragraph| {
        for index in 0..=3 {
            let position = Offset::new(index as f32 * FONT_SIZE + 1.0, FONT_SIZE / 2.0);
            assert_eq!(paragraph.get_offset_for_position(position), index, "{stack}");
        }
    });
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getOffsetForPosition_rtl
#[test]
fn a_position_in_the_sample_font_right_to_left_counts_from_the_right() {
    let text = "\u{5D0}\u{5D1}\u{5D2}";
    plain(text, 3.0 * FONT_SIZE, |stack, paragraph| {
        for index in 0..3 {
            let position = Offset::new(index as f32 * FONT_SIZE + 1.0, FONT_SIZE / 2.0);
            // Bytes here, two to a letter.
            let expected = (3 - index) * 2;
            assert_eq!(paragraph.get_offset_for_position(position), expected, "{stack}");
        }
    });
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getBoundingBox_ltr_singleLine
#[test]
fn a_characters_box_in_the_sample_font_is_a_square_em() {
    plain("abc", 3.0 * FONT_SIZE, |stack, paragraph| {
        for index in 0..3 {
            let left = index as f32 * FONT_SIZE;
            let expected = Rect::new(left, 0.0, left + FONT_SIZE, FONT_SIZE);
            assert_eq!(paragraph.get_bounding_box(index), expected, "{stack}");
        }
    });
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getBoundingBox_rtl_singleLine
#[test]
fn a_characters_box_right_to_left_in_the_sample_font_is_from_the_right() {
    let text = "\u{5D0}\u{5D1}\u{5D2}";
    plain(text, 3.0 * FONT_SIZE, |stack, paragraph| {
        for index in 0..3 {
            let right = 3.0 * FONT_SIZE - index as f32 * FONT_SIZE;
            let expected = Rect::new(right - FONT_SIZE, 0.0, right, FONT_SIZE);
            assert_eq!(paragraph.get_bounding_box(index * 2), expected, "{stack}");
            assert_eq!(
                paragraph.get_bidi_run_direction(index * 2),
                ResolvedTextDirection::Rtl,
                "{stack}"
            );
        }
    });
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getLineBaseline
#[test]
fn every_line_in_the_sample_font_is_one_em_high() {
    plain("abc\ndef\nghi", 3.0 * FONT_SIZE, |stack, paragraph| {
        for line in 0..3 {
            assert_eq!(paragraph.get_line_height(line), FONT_SIZE, "{stack}");
            assert_eq!(paragraph.get_line_top(line), line as f32 * FONT_SIZE, "{stack}");
            assert_baseline(
                paragraph.get_line_baseline(line),
                (line as f32 + 0.8) * FONT_SIZE,
                stack,
            );
            assert_eq!(paragraph.get_line_width(line), 3.0 * FONT_SIZE, "{stack}");
        }
    });
}

#[test]
fn a_space_in_the_sample_font_is_one_em_and_ends_a_word() {
    plain("abc def", 7.0 * FONT_SIZE, |stack, paragraph| {
        assert_eq!(paragraph.max_intrinsic_width(), 7.0 * FONT_SIZE, "{stack}");
        assert_eq!(paragraph.get_word_boundary(1), TextRange::new(0, 3), "{stack}");
    });
}
