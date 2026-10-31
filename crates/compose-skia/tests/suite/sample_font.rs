//! Exact expectations on the real stack: Compose's `ParagraphIntegrationTest` with a font every
//! glyph of which is one em wide, on SkParagraph and on the fixed measurer with the same metrics,
//! so the fixed measurer's tests say what the real one does.

use std::rc::Rc;

use compose::test::basic_measure_font;
use compose::ui::platform::{FixedFontMetrics, FixedTextMeasurer, TextLayoutOptions, TextMeasurer};
use compose::ui::{
    Constraints, Density, Offset, Paragraph, Rect, ResolvedTextDirection, TextDirection,
    TextOverflow, TextRange, TextStyle, UnitExt,
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

fn ellipsized(overflow: TextOverflow, max_lines: u32) -> TextLayoutOptions {
    TextLayoutOptions { overflow, max_lines, ..TextLayoutOptions::default() }
}

/// `text` laid out on each stack at `width`, at most `height` high.
fn at_height(
    text: &str,
    options: TextLayoutOptions,
    width: f32,
    height: f32,
    check: impl Fn(&str, &dyn Paragraph),
) {
    let layout = |measurer: &mut dyn TextMeasurer| {
        let constraints = Constraints::new(width as i32, width as i32, 0, height as i32);
        let handle = measurer.layout(text, &style(), options, constraints, &Density::new(1.0));
        let paragraph = measurer.paragraph(handle);
        measurer.release(handle);
        paragraph
    };
    let mut fixed = FixedTextMeasurer::with_metrics(FixedFontMetrics::SAMPLE_FONT);
    check("the fixed measurer", &*layout(&mut fixed));
    let mut skia = SkiaTextMeasurer::new();
    check("SkParagraph", &*layout(&mut skia));
}

/// The text of Compose's height cases: four lines at this width.
const THIS_IS_A_TEXT: &str = "This is a text";

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testEllipsis_withLimitedHeightFitAllLines_doesNotEllipsis
#[test]
fn an_ellipsis_with_room_for_every_line_is_not_there() {
    let options = ellipsized(TextOverflow::Ellipsis, u32::MAX);
    at_height(THIS_IS_A_TEXT, options, 4.0 * FONT_SIZE, 6.0 * FONT_SIZE, |stack, paragraph| {
        for line in 0..paragraph.line_count() {
            assert!(!paragraph.is_line_ellipsized(line), "{stack}: {line}");
        }
    });
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testEllipsis_withLimitedHeight_doesEllipsis
#[test]
fn an_ellipsis_ends_the_last_line_the_height_has_room_for() {
    let options = ellipsized(TextOverflow::Ellipsis, u32::MAX);
    at_height(THIS_IS_A_TEXT, options, 4.0 * FONT_SIZE, 2.2 * FONT_SIZE, |stack, paragraph| {
        assert_eq!(paragraph.line_count(), 2, "{stack}");
        assert!(paragraph.is_line_ellipsized(1), "{stack}");
        assert!(paragraph.did_exceed_max_lines(), "{stack}");
        assert_eq!(paragraph.height(), 2.0 * FONT_SIZE, "{stack}");
    });
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testEllipsis_withLimitedHeight_overflowNotEllipsis_doesNotEllipsis
#[test]
fn a_clipped_text_is_not_ellipsized_at_the_height() {
    let options = ellipsized(TextOverflow::Clip, u32::MAX);
    at_height(THIS_IS_A_TEXT, options, 4.0 * FONT_SIZE, 2.2 * FONT_SIZE, |stack, paragraph| {
        assert!(paragraph.line_count() > 2, "{stack}");
        for line in 0..paragraph.line_count() {
            assert!(!paragraph.is_line_ellipsized(line), "{stack}: {line}");
        }
    });
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testEllipsis_withMaxLinesMoreThanTextLines_andLimitedHeight_doesEllipsis
#[test]
fn the_height_decides_over_more_maximum_lines() {
    let options = ellipsized(TextOverflow::Ellipsis, 5);
    at_height(THIS_IS_A_TEXT, options, 4.0 * FONT_SIZE, 2.2 * FONT_SIZE, |stack, paragraph| {
        assert_eq!(paragraph.line_count(), 2, "{stack}");
        assert!(paragraph.is_line_ellipsized(1), "{stack}");
    });
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testEllipsis_withMaxLines_andLimitedHeight_doesEllipsis
#[test]
fn the_maximum_lines_decide_with_room_in_the_height() {
    let options = ellipsized(TextOverflow::Ellipsis, 2);
    at_height(THIS_IS_A_TEXT, options, 4.0 * FONT_SIZE, 4.0 * FONT_SIZE, |stack, paragraph| {
        assert_eq!(paragraph.line_count(), 2, "{stack}");
        assert!(paragraph.is_line_ellipsized(1), "{stack}");
    });
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testEllipsis_whenHeightAllowsForZeroLines_doesEllipsis
#[test]
fn a_height_for_no_line_still_ellipsizes_one() {
    let options = ellipsized(TextOverflow::Ellipsis, u32::MAX);
    at_height(THIS_IS_A_TEXT, options, 4.0 * FONT_SIZE, FONT_SIZE / 4.0, |stack, paragraph| {
        assert!(paragraph.did_exceed_max_lines(), "{stack}");
        assert_eq!(paragraph.line_count(), 1, "{stack}");
        assert!(paragraph.is_line_ellipsized(0), "{stack}");
    });
}

fn a_box(left: f32, width: f32) -> Rect {
    Rect::new(left * FONT_SIZE, 0.0, (left + width) * FONT_SIZE, FONT_SIZE)
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testEllipsis_withMaxLinesOne_doesStartEllipsis
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getLineStartEllipsisCount
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/TextLayoutIsLineEllipsizedTest.kt, ellipsizeEnd_start_ltr
#[test]
fn a_start_ellipsis_shows_the_end_of_the_line() {
    let options = ellipsized(TextOverflow::StartEllipsis, 1);
    on_each_stack("abcde", options, 3.0 * FONT_SIZE, |stack, paragraph| {
        assert_eq!(paragraph.line_count(), 1, "{stack}");
        assert!(paragraph.is_line_ellipsized(0), "{stack}");
        assert_eq!(paragraph.get_line_start(0), 0, "{stack}");
        assert_eq!(paragraph.get_line_end(0, false), 5, "{stack}");
        // Compose's visible end stops at an ellipsis at the end only.
        assert_eq!(paragraph.get_line_end(0, true), 5, "{stack}");
        // The "…", where the first of three characters hidden is, the other two nothing, and
        // the two characters left.
        assert_eq!(paragraph.get_bounding_box(0), a_box(0.0, 1.0), "{stack}");
        assert_eq!(paragraph.get_bounding_box(1).width(), 0.0, "{stack}");
        assert_eq!(paragraph.get_bounding_box(2).width(), 0.0, "{stack}");
        assert_eq!(paragraph.get_bounding_box(3), a_box(1.0, 1.0), "{stack}");
        assert_eq!(paragraph.get_bounding_box(4), a_box(2.0, 1.0), "{stack}");
        assert_eq!(paragraph.max_intrinsic_width(), 5.0 * FONT_SIZE, "{stack}");
        assert!(paragraph.did_exceed_max_lines(), "{stack}");
    });
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testEllipsis_withMaxLinesOne_doesMiddleEllipsis
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getLineMiddleEllipsisCount
#[test]
fn a_middle_ellipsis_shows_both_ends_of_the_line() {
    let options = ellipsized(TextOverflow::MiddleEllipsis, 1);
    on_each_stack("abcde", options, 3.0 * FONT_SIZE, |stack, paragraph| {
        assert_eq!(paragraph.line_count(), 1, "{stack}");
        assert!(paragraph.is_line_ellipsized(0), "{stack}");
        assert_eq!(paragraph.get_line_end(0, false), 5, "{stack}");
        assert_eq!(paragraph.get_line_end(0, true), 5, "{stack}");
        assert_eq!(paragraph.get_bounding_box(0), a_box(0.0, 1.0), "{stack}");
        assert_eq!(paragraph.get_bounding_box(1), a_box(1.0, 1.0), "{stack}");
        assert_eq!(paragraph.get_bounding_box(2).width(), 0.0, "{stack}");
        assert_eq!(paragraph.get_bounding_box(3).width(), 0.0, "{stack}");
        assert_eq!(paragraph.get_bounding_box(4), a_box(2.0, 1.0), "{stack}");
    });
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/TextLayoutIsLineEllipsizedTest.kt, ellipsizeEnd_start_rtl
#[test]
fn a_start_ellipsis_right_to_left_shows_the_end_on_the_left() {
    let text = "\u{5D0}\u{5D1}\u{5D2}\u{5D3}\u{5D4}";
    let options = ellipsized(TextOverflow::StartEllipsis, 1);
    on_each_stack(text, options, 3.0 * FONT_SIZE, |stack, paragraph| {
        assert!(paragraph.is_line_ellipsized(0), "{stack}");
        // The "…" at the right, in the place of the first letter, the last on the left.
        assert_eq!(paragraph.get_bounding_box(0), a_box(2.0, 1.0), "{stack}");
        assert_eq!(paragraph.get_bounding_box(8), a_box(0.0, 1.0), "{stack}");
    });
}

#[test]
fn a_start_ellipsis_that_fits_is_not_there() {
    for overflow in [TextOverflow::StartEllipsis, TextOverflow::MiddleEllipsis] {
        on_each_stack("abc", ellipsized(overflow, 1), 3.0 * FONT_SIZE, |stack, paragraph| {
            assert!(!paragraph.is_line_ellipsized(0), "{stack}");
            assert!(!paragraph.did_exceed_max_lines(), "{stack}");
            assert_eq!(paragraph.get_line_end(0, true), 3, "{stack}");
        });
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/TextLayoutLineVisibleEndTest.kt, excludesLineBreak_whenMaxLinesPresent_withEllipsisStart
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/TextLayoutLineVisibleEndTest.kt, excludesLineBreak_whenMaxLinesPresent_withEllipsisMiddle
#[test]
fn a_line_feed_past_the_first_line_is_not_visible_and_ellipsizes_nothing_with_room() {
    for overflow in [TextOverflow::StartEllipsis, TextOverflow::MiddleEllipsis] {
        on_each_stack("abc\ndef", ellipsized(overflow, 1), 10.0 * FONT_SIZE, |stack, paragraph| {
            assert_eq!(paragraph.line_count(), 1, "{stack}");
            assert_eq!(paragraph.get_line_end(0, true), 3, "{stack}");
            assert!(!paragraph.is_line_ellipsized(0), "{stack}");
        });
    }
}

#[test]
fn a_start_ellipsis_falls_back_to_a_clip_on_many_lines() {
    for overflow in [TextOverflow::StartEllipsis, TextOverflow::MiddleEllipsis] {
        on_each_stack("abcdef", ellipsized(overflow, 3), 3.0 * FONT_SIZE, |stack, paragraph| {
            assert_eq!(paragraph.line_count(), 2, "{stack}");
            assert!(!paragraph.is_line_ellipsized(0), "{stack}");
            assert!(!paragraph.is_line_ellipsized(1), "{stack}");
        });
    }
}

const SIX_EMOJI: &str = "\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}";

#[test]
fn a_start_ellipsis_counts_a_surrogate_pair_as_one_character() {
    let options = ellipsized(TextOverflow::StartEllipsis, 1);
    on_each_stack(SIX_EMOJI, options, 4.0 * FONT_SIZE, |stack, paragraph| {
        assert!(paragraph.is_line_ellipsized(0), "{stack}");
        assert_eq!(paragraph.get_bounding_box(0), a_box(0.0, 1.0), "{stack}");
        assert_eq!(paragraph.get_bounding_box(4).width(), 0.0, "{stack}");
        assert_eq!(paragraph.get_bounding_box(8).width(), 0.0, "{stack}");
        assert_eq!(paragraph.get_bounding_box(12), a_box(1.0, 1.0), "{stack}");
        assert_eq!(paragraph.get_bounding_box(20), a_box(3.0, 1.0), "{stack}");
    });
}

#[test]
fn a_middle_ellipsis_counts_a_surrogate_pair_as_one_character() {
    let options = ellipsized(TextOverflow::MiddleEllipsis, 1);
    on_each_stack(SIX_EMOJI, options, 4.0 * FONT_SIZE, |stack, paragraph| {
        assert!(paragraph.is_line_ellipsized(0), "{stack}");
        assert_eq!(paragraph.get_bounding_box(4), a_box(1.0, 1.0), "{stack}");
        assert_eq!(paragraph.get_bounding_box(8), a_box(2.0, 1.0), "{stack}");
        assert_eq!(paragraph.get_bounding_box(12).width(), 0.0, "{stack}");
        assert_eq!(paragraph.get_bounding_box(20), a_box(3.0, 1.0), "{stack}");
    });
}

#[test]
fn an_inner_ellipsis_keeps_a_letter_with_its_combining_mark() {
    // Twenty: a letter with a mark the font hasn't falls back to one of the system's, narrower.
    let text = "e\u{301}".repeat(20);
    for overflow in [TextOverflow::StartEllipsis, TextOverflow::MiddleEllipsis] {
        on_each_stack(&text, ellipsized(overflow, 1), 4.0 * FONT_SIZE, |stack, paragraph| {
            assert!(paragraph.is_line_ellipsized(0), "{stack}: {overflow:?}");
            // None measured as nothing, none cut from its mark: it fills the width.
            let width = paragraph.get_line_width(0);
            assert!(width <= 4.0 * FONT_SIZE + 0.01, "{stack}: {overflow:?}: {width}");
            assert!(width >= 3.0 * FONT_SIZE, "{stack}: {overflow:?}: {width}");
        });
    }
}

#[test]
fn a_texts_intrinsics_do_not_depend_on_the_width_breaking_it() {
    // Ten letters in nine and a half: the broken word's rest is shorter than the width.
    plain("abcdefghij", 9.5 * FONT_SIZE, |stack, paragraph| {
        assert_eq!(paragraph.line_count(), 2, "{stack}");
        assert_eq!(paragraph.min_intrinsic_width(), 10.0 * FONT_SIZE, "{stack}");
        assert_eq!(paragraph.max_intrinsic_width(), 10.0 * FONT_SIZE, "{stack}");
    });
}

#[test]
fn a_height_exactly_a_number_of_lines_has_room_for_them() {
    let options = ellipsized(TextOverflow::Ellipsis, u32::MAX);
    // Two lines in two lines' height: nothing to end.
    at_height("abcdefgh", options, 4.0 * FONT_SIZE, 2.0 * FONT_SIZE, |stack, paragraph| {
        assert_eq!(paragraph.line_count(), 2, "{stack}");
        assert!(!paragraph.is_line_ellipsized(1), "{stack}");
        assert!(!paragraph.did_exceed_max_lines(), "{stack}");
    });
    // Three, in two: the second ends them.
    at_height("abcdefghijkl", options, 4.0 * FONT_SIZE, 2.0 * FONT_SIZE, |stack, paragraph| {
        assert_eq!(paragraph.line_count(), 2, "{stack}");
        assert!(paragraph.is_line_ellipsized(1), "{stack}");
        assert_eq!(paragraph.height(), 2.0 * FONT_SIZE, "{stack}");
    });
    // A pixel short of two: the first.
    at_height("abcdefgh", options, 4.0 * FONT_SIZE, 2.0 * FONT_SIZE - 1.0, |stack, paragraph| {
        assert_eq!(paragraph.line_count(), 1, "{stack}");
        assert!(paragraph.is_line_ellipsized(0), "{stack}");
    });
    // One line's height with one line's maximum isn't asked twice.
    let one = ellipsized(TextOverflow::Ellipsis, 1);
    at_height("abcdefgh", one, 4.0 * FONT_SIZE, FONT_SIZE / 4.0, |stack, paragraph| {
        assert_eq!(paragraph.line_count(), 1, "{stack}");
    });
}
