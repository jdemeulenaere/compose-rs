//! `Paragraph`'s queries, adapted from Compose's `ParagraphIntegrationTest`: laid out by the
//! fixed measurer with Compose's test font metrics (`FixedFontMetrics::SAMPLE_FONT`: one em
//! wide, lines one em high, the baseline at 0.8 em), as Compose's tests lay out with its
//! `sample_font`. A Compose paragraph laid out at a width is the measurer's paragraph laid out
//! at that fixed width.

use std::rc::Rc;

use compose_ui::platform::{FixedFontMetrics, FixedTextMeasurer, TextLayoutOptions, TextMeasurer};
use compose_ui::{
    AnnotatedString, Constraints, Density, LineHeightAlignment, LineHeightStyle, LineHeightTrim,
    Offset, Paragraph, Rect, ResolvedTextDirection, SpanStyle, TextAlign, TextDirection,
    TextGeometricTransform, TextIndent, TextOverflow, TextRange, TextStyle, TextUnit, UnitExt,
};

/// How a test lays its paragraph out: Compose's `simpleParagraph` arguments.
struct Setup {
    style: TextStyle,
    width: Option<f32>,
    max_lines: u32,
    overflow: TextOverflow,
    density: Density,
}

impl Default for Setup {
    fn default() -> Self {
        Setup {
            style: TextStyle::DEFAULT,
            width: None,
            max_lines: u32::MAX,
            overflow: TextOverflow::Clip,
            density: Density::new(1.0),
        }
    }
}

/// `text` laid out as `setup` says: at `width` exactly, or unbounded at its own width.
fn lay_out(text: impl Into<AnnotatedString>, setup: Setup) -> Rc<dyn Paragraph> {
    let mut measurer = FixedTextMeasurer::with_metrics(FixedFontMetrics::SAMPLE_FONT);
    let constraints = match setup.width {
        Some(width) => {
            let width = width.ceil() as i32;
            Constraints::new(width, width, 0, Constraints::INFINITY)
        }
        None => Constraints::unbounded(),
    };
    let options = TextLayoutOptions {
        max_lines: setup.max_lines,
        overflow: setup.overflow,
        ..TextLayoutOptions::default()
    };
    let handle =
        measurer.layout_annotated(&text.into(), &setup.style, options, constraints, &setup.density);
    measurer.paragraph(handle)
}

/// `text` in `style` at `width`.
fn paragraph(text: &str, style: TextStyle, width: Option<f32>) -> Rc<dyn Paragraph> {
    lay_out(text.to_string(), Setup { style, width, ..Setup::default() })
}

/// A style of `size` sp.
fn sized(size: f32) -> TextStyle {
    TextStyle { font_size: size.sp(), ..TextStyle::DEFAULT }
}

/// A style of `size` sp in `direction`.
fn directed(size: f32, direction: TextDirection) -> TextStyle {
    TextStyle { text_direction: Some(direction), ..sized(size) }
}

/// The number of characters of `text` (its length in Compose's tests, which are all in the
/// basic plane).
fn chars(text: &str) -> usize {
    text.chars().count()
}

/// The byte offset of character `index` of `text`.
fn at(text: &str, index: usize) -> usize {
    text.char_indices().nth(index).map_or(text.len(), |(offset, _)| offset)
}

const HEBREW: &str = "\u{05D0}\u{05D1}\u{05D2}";

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, empty_string
#[test]
fn an_empty_paragraph_is_one_line_high() {
    let paragraph = paragraph("", sized(50.0), Some(100.0));
    assert_eq!(paragraph.width(), 100.0);
    assert_eq!(paragraph.height(), 50.0);
    assert_eq!(paragraph.first_baseline(), 40.0);
    assert_eq!(paragraph.last_baseline(), 40.0);
    assert_eq!(paragraph.max_intrinsic_width(), 0.0);
    assert_eq!(paragraph.min_intrinsic_width(), 0.0);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, single_line_default_values
#[test]
fn a_single_line_has_the_font_metrics() {
    for text in ["xyz", HEBREW] {
        let paragraph = paragraph(text, sized(50.0), Some(200.0));
        assert_eq!(paragraph.width(), 200.0, "{text}");
        assert_eq!(paragraph.height(), 50.0, "{text}");
        assert_eq!(paragraph.first_baseline(), 40.0, "{text}");
        assert_eq!(paragraph.last_baseline(), 40.0, "{text}");
        assert_eq!(paragraph.max_intrinsic_width(), 50.0 * chars(text) as f32, "{text}");
        assert_eq!(paragraph.min_intrinsic_width(), 50.0 * chars(text) as f32, "{text}");
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, line_break_default_values
#[test]
fn a_line_break_adds_a_line() {
    for text in ["abcdef", "\u{05D0}\u{05D1}\u{05D2}\u{05D3}\u{05D4}\u{05D5}"] {
        let paragraph = paragraph(text, sized(50.0), Some(150.0));
        assert_eq!(paragraph.width(), 150.0, "{text}");
        assert_eq!(paragraph.height(), 100.0, "{text}");
        assert_eq!(paragraph.first_baseline(), 40.0, "{text}");
        assert_eq!(paragraph.last_baseline(), 90.0, "{text}");
        assert_eq!(paragraph.max_intrinsic_width(), 300.0, "{text}");
        assert_eq!(paragraph.min_intrinsic_width(), 300.0, "{text}");
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, newline_default_values
#[test]
fn a_line_feed_adds_a_line() {
    for text in ["abc\ndef", "\u{05D0}\u{05D1}\u{05D2}\n\u{05D3}\u{05D4}\u{05D5}"] {
        let paragraph = paragraph(text, sized(50.0), Some(150.0));
        assert_eq!(paragraph.width(), 150.0, "{text}");
        assert_eq!(paragraph.height(), 100.0, "{text}");
        assert_eq!(paragraph.first_baseline(), 40.0, "{text}");
        assert_eq!(paragraph.last_baseline(), 90.0, "{text}");
        assert_eq!(paragraph.max_intrinsic_width(), 150.0, "{text}");
        assert_eq!(paragraph.min_intrinsic_width(), 150.0, "{text}");
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, newline_and_line_break_default_values
#[test]
fn line_feeds_and_line_breaks_add_lines() {
    for text in ["abc\ndef", "\u{05D0}\u{05D1}\u{05D2}\n\u{05D3}\u{05D4}\u{05D5}"] {
        let paragraph = paragraph(text, sized(50.0), Some(100.0));
        assert_eq!(paragraph.width(), 100.0, "{text}");
        assert_eq!(paragraph.height(), 200.0, "{text}");
        assert_eq!(paragraph.first_baseline(), 40.0, "{text}");
        assert_eq!(paragraph.last_baseline(), 190.0, "{text}");
        assert_eq!(paragraph.max_intrinsic_width(), 150.0, "{text}");
        assert_eq!(paragraph.min_intrinsic_width(), 150.0, "{text}");
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getOffsetForPosition_ltr
#[test]
fn the_offset_for_a_position_left_to_right() {
    let paragraph = paragraph("abc", sized(50.0), Some(150.0));
    for i in 0..=3 {
        let position = Offset::new(i as f32 * 50.0 + 1.0, 25.0);
        assert_eq!(paragraph.get_offset_for_position(position), i, "at {i}");
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getOffsetForPosition_rtl
#[test]
fn the_offset_for_a_position_right_to_left() {
    let paragraph = paragraph(HEBREW, sized(50.0), Some(150.0));
    for i in 0..=3 {
        let position = Offset::new(i as f32 * 50.0 + 1.0, 25.0);
        assert_eq!(paragraph.get_offset_for_position(position), at(HEBREW, 3 - i), "at {i}");
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getOffsetForPosition_ltr_multiline
#[test]
fn the_offset_for_a_position_on_a_second_line_left_to_right() {
    let paragraph = paragraph("abcdef", sized(50.0), Some(150.0));
    for i in 0..=3 {
        let position = Offset::new(i as f32 * 50.0 + 1.0, 75.0);
        assert_eq!(paragraph.get_offset_for_position(position), i + 3, "at {i}");
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getOffsetForPosition_rtl_multiline
#[test]
fn the_offset_for_a_position_on_a_second_line_right_to_left() {
    let text = "\u{05D0}\u{05D1}\u{05D2}\u{05D3}\u{05D4}\u{05D5}";
    let paragraph = paragraph(text, sized(50.0), Some(150.0));
    for i in 0..=3 {
        let position = Offset::new(i as f32 * 50.0 + 1.0, 75.0);
        assert_eq!(paragraph.get_offset_for_position(position), at(text, 6 - i), "at {i}");
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getOffsetForPosition_ltr_width_outOfBounds
#[test]
fn the_offset_for_a_position_past_the_width_is_kept_in_the_line() {
    let paragraph = paragraph("abc", sized(50.0), Some(150.0));
    assert_eq!(paragraph.get_offset_for_position(Offset::new(300.0, 25.0)), 3);
    assert_eq!(paragraph.get_offset_for_position(Offset::new(-50.0, 25.0)), 0);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getOffsetForPosition_ltr_height_outOfBounds
#[test]
fn the_offset_for_a_position_past_the_height_is_on_the_nearest_line() {
    let paragraph = paragraph("abc", sized(50.0), Some(150.0));
    assert_eq!(paragraph.get_offset_for_position(Offset::new(25.0, 300.0)), 0);
    assert_eq!(paragraph.get_offset_for_position(Offset::new(25.0, -50.0)), 0);
}

/// "abcdefgh" (or 8 Hebrew letters) at 20 sp in 4 lines 30 sp high.
fn four_lines(text: &str) -> Rc<dyn Paragraph> {
    let style = TextStyle { line_height: 30.sp(), ..sized(20.0) };
    paragraph(text, style, Some(40.0))
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getLineForVerticalPosition_ltr
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getLineForVerticalPosition_rtl
#[test]
fn the_line_for_a_vertical_position() {
    let hebrew = "\u{05D0}\u{05D1}\u{05D2}\u{05D3}\u{05D4}\u{05D5}\u{05D6}\u{05D7}";
    for text in ["abcdefgh", hebrew] {
        let paragraph = four_lines(text);
        assert_eq!(paragraph.line_count(), 4);
        for line in 0..4 {
            let position = line as f32 * 30.0 + 1.0;
            assert_eq!(paragraph.get_line_for_vertical_position(position), line, "{text}");
        }
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getLineForVerticalPosition_ltr_height_outOfBounds
#[test]
fn the_line_for_a_vertical_position_past_the_lines_is_the_nearest() {
    let paragraph = four_lines("abcdefgh");
    assert_eq!(paragraph.get_line_for_vertical_position(240.0), 3);
    assert_eq!(paragraph.get_line_for_vertical_position(-30.0), 0);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getLineForVerticalPosition_ltr_lineTopCenterBottom_paddingFalse
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getLineForVerticalPosition_ltr_lineTopCenterBottom_paddingTrue
#[test]
fn a_lines_top_center_and_bottom_are_on_it() {
    let paragraph = lay_out(
        "ab\ncde\n\nfg",
        Setup { style: sized(14.0), density: Density::new(3.5), ..Setup::default() },
    );
    assert_eq!(paragraph.line_count(), 4);
    for line in 0..4 {
        let (top, bottom) = (paragraph.get_line_top(line), paragraph.get_line_bottom(line));
        assert_eq!(paragraph.get_line_for_vertical_position(top), line);
        assert_eq!(paragraph.get_line_for_vertical_position((top + bottom) / 2.0), line);
        assert_eq!(paragraph.get_line_for_vertical_position(bottom - 1.0), line);
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getBoundingBox_ltr_singleLine
#[test]
fn bounding_boxes_left_to_right() {
    let paragraph = paragraph("abc", sized(50.0), Some(150.0));
    for i in 0..3 {
        let left = i as f32 * 50.0;
        assert_eq!(paragraph.get_bounding_box(i), Rect::new(left, 0.0, left + 50.0, 50.0));
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getBoundingBox_rtl_singleLine
#[test]
fn bounding_boxes_right_to_left() {
    let paragraph = paragraph(HEBREW, sized(50.0), Some(150.0));
    for c in 0..3 {
        let left = (2 - c) as f32 * 50.0;
        assert_eq!(
            paragraph.get_bounding_box(at(HEBREW, c)),
            Rect::new(left, 0.0, left + 50.0, 50.0)
        );
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getBoundingBox_ltr_multiLines
#[test]
fn bounding_boxes_on_a_second_line_left_to_right() {
    let paragraph = paragraph("abcdef", sized(50.0), Some(150.0));
    for i in 0..3 {
        let left = i as f32 * 50.0;
        assert_eq!(paragraph.get_bounding_box(i + 3), Rect::new(left, 50.0, left + 50.0, 100.0));
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getBoundingBox_rtl_multiLines
#[test]
fn bounding_boxes_on_a_second_line_right_to_left() {
    let text = "\u{05D0}\u{05D1}\u{05D2}\u{05D3}\u{05D4}\u{05D5}";
    let paragraph = paragraph(text, sized(50.0), Some(150.0));
    for i in 0..3 {
        let left = (2 - i) as f32 * 50.0;
        assert_eq!(
            paragraph.get_bounding_box(at(text, i + 3)),
            Rect::new(left, 50.0, left + 50.0, 100.0)
        );
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getBoundingBox_ltr_multiLines_spaceAtTheEndOfLine
#[test]
fn the_bounding_box_of_a_lines_trailing_space_is_empty_left_to_right() {
    let paragraph = paragraph("abc def", sized(50.0), Some(200.0));
    assert_eq!(paragraph.get_bounding_box(3), Rect::new(150.0, 0.0, 150.0, 50.0));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getBoundingBox_rtl_multiLines_spaceAtTheEndOfLine
#[test]
fn the_bounding_box_of_a_lines_trailing_space_is_empty_right_to_left() {
    let text = "\u{05D0}\u{05D1}\u{05D2} \u{05D3}\u{05D4}\u{05D5}";
    let paragraph = paragraph(text, sized(50.0), Some(200.0));
    assert_eq!(paragraph.get_bounding_box(at(text, 3)), Rect::new(50.0, 0.0, 50.0, 50.0));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getBoundingBox_ltr_multiLines_lineFeedEllipsized_maxLines
#[test]
fn the_bounding_box_of_an_ellipsized_character_is_empty_left_to_right() {
    let setup = Setup {
        style: sized(50.0),
        width: Some(150.0),
        overflow: TextOverflow::Ellipsis,
        max_lines: 1,
        ..Setup::default()
    };
    let paragraph = lay_out("abc def\ndef", setup);
    assert_eq!(paragraph.get_bounding_box(9), Rect::new(150.0, 0.0, 150.0, 50.0));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getBoundingBox_rtl_multiLines_lineFeedEllipsized_maxLines
#[test]
fn the_bounding_box_of_an_ellipsized_character_is_empty_right_to_left() {
    let text = "\u{05D0}\u{05D1}\u{05D2} \u{05D3}\u{05D4}\u{05D5}\n\u{05D0}\u{05D1}\u{05D2}";
    let setup = Setup {
        style: sized(50.0),
        width: Some(150.0),
        overflow: TextOverflow::Ellipsis,
        max_lines: 1,
        ..Setup::default()
    };
    let paragraph = lay_out(text, setup);
    assert_eq!(paragraph.get_bounding_box(at(text, 9)), Rect::new(0.0, 0.0, 0.0, 50.0));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getBoundingBox_ltr_textPosition_larger_than_length_throw_exception
#[test]
#[should_panic(expected = "out of bounds")]
fn a_bounding_box_past_the_text_panics() {
    paragraph("abc", sized(50.0), Some(150.0)).get_bounding_box(4);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getCursorRect_larger_than_length_throw_exception
#[test]
#[should_panic(expected = "out of bounds")]
fn a_cursor_past_the_text_panics() {
    paragraph("abc", sized(50.0), Some(150.0)).get_cursor_rect(4);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getCursorRect_ltr_singleLine
#[test]
fn cursors_left_to_right() {
    let paragraph = paragraph("abc", sized(50.0), Some(150.0));
    for i in 0..3 {
        let x = i as f32 * 50.0;
        assert_eq!(paragraph.get_cursor_rect(i), Rect::new(x, 0.0, x, 50.0));
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getCursorRect_ltr_multiLines
#[test]
fn cursors_on_two_lines_left_to_right() {
    let paragraph = paragraph("abcdef", sized(50.0), Some(150.0));
    for i in 0..6 {
        let x = (i % 3) as f32 * 50.0;
        let top = (i / 3) as f32 * 50.0;
        assert_eq!(paragraph.get_cursor_rect(i), Rect::new(x, top, x, top + 50.0), "at {i}");
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getCursorRect_ltr_newLine
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getCursorRect_ltr_newLine_last_char
#[test]
fn cursors_around_a_line_feed_left_to_right() {
    for text in ["abc\ndef", "abc\n"] {
        let paragraph = paragraph(text, sized(50.0), None);
        assert_eq!(paragraph.get_cursor_rect(3), Rect::new(150.0, 0.0, 150.0, 50.0), "{text}");
        assert_eq!(paragraph.get_cursor_rect(4), Rect::new(0.0, 50.0, 0.0, 100.0), "{text}");
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getCursorRect_rtl_singleLine
#[test]
fn cursors_right_to_left() {
    let paragraph = paragraph(HEBREW, sized(50.0), Some(150.0));
    for i in 0..3 {
        let x = (3 - i) as f32 * 50.0;
        assert_eq!(paragraph.get_cursor_rect(at(HEBREW, i)), Rect::new(x, 0.0, x, 50.0));
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getCursorRect_rtl_multiLines
#[test]
fn cursors_on_two_lines_right_to_left() {
    let text = "\u{05D0}\u{05D1}\u{05D2}\u{05D0}\u{05D1}\u{05D2}";
    let paragraph = paragraph(text, sized(50.0), Some(150.0));
    for i in 0..6 {
        let x = (3 - i % 3) as f32 * 50.0;
        let top = (i / 3) as f32 * 50.0;
        assert_eq!(
            paragraph.get_cursor_rect(at(text, i)),
            Rect::new(x, top, x, top + 50.0),
            "at {i}"
        );
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getCursorRect_rtl_newLine
#[test]
fn cursors_around_a_line_feed_right_to_left() {
    let text = "\u{05D0}\u{05D1}\u{05D2}\n\u{05D0}\u{05D1}\u{05D2}";
    let paragraph = paragraph(text, sized(50.0), Some(150.0));
    assert_eq!(paragraph.get_cursor_rect(at(text, 3)), Rect::new(0.0, 0.0, 0.0, 50.0));
    assert_eq!(paragraph.get_cursor_rect(at(text, 4)), Rect::new(150.0, 50.0, 150.0, 100.0));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getCursorRect_rtl_newLine_last_char
#[test]
fn a_cursor_after_a_final_line_feed_is_left_to_right() {
    let text = "\u{05D0}\u{05D1}\u{05D2}\n";
    let paragraph = paragraph(text, sized(50.0), Some(150.0));
    assert_eq!(paragraph.get_cursor_rect(at(text, 3)), Rect::new(0.0, 0.0, 0.0, 50.0));
    assert_eq!(paragraph.get_cursor_rect(at(text, 4)), Rect::new(0.0, 50.0, 0.0, 100.0));
}

/// The horizontal positions of `text`'s character boundaries in `style`, laid out at its
/// width: the primary ones or the others.
fn horizontals(text: &str, style: TextStyle, primary: bool) -> Vec<f32> {
    let width = chars(text) as f32 * 50.0;
    let paragraph = paragraph(text, style, Some(width));
    (0..=chars(text)).map(|i| paragraph.get_horizontal_position(at(text, i), primary)).collect()
}

const MIXED: &str = "abc\u{05D0}\u{05D1}\u{05D2}";

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getHorizontalPositionForOffset_primary_ltr_singleLine_textDirectionDefault
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getHorizontalPositionForOffset_notPrimary_ltr_singleLine_textDirectionDefault
#[test]
fn horizontal_positions_left_to_right() {
    for primary in [true, false] {
        assert_eq!(horizontals("abc", sized(50.0), primary), [0.0, 50.0, 100.0, 150.0]);
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getHorizontalPositionForOffset_primary_rtl_singleLine_textDirectionDefault
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getHorizontalPositionForOffset_notPrimary_rtl_singleLine_textDirectionDefault
#[test]
fn horizontal_positions_right_to_left() {
    for primary in [true, false] {
        assert_eq!(horizontals(HEBREW, sized(50.0), primary), [150.0, 100.0, 50.0, 0.0]);
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getHorizontalPositionForOffset_primary_Bidi_singleLine_textDirectionDefault
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getHorizontalPositionForOffset_primary_Bidi_singleLine_textDirectionLtr
#[test]
fn primary_horizontal_positions_of_bidi_text_left_to_right() {
    let expected = [0.0, 50.0, 100.0, 150.0, 250.0, 200.0, 300.0];
    assert_eq!(horizontals(MIXED, sized(50.0), true), expected);
    assert_eq!(horizontals(MIXED, directed(50.0, TextDirection::Ltr), true), expected);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getHorizontalPositionForOffset_notPrimary_Bidi_singleLine_textDirectionDefault
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getHorizontalPositionForOffset_notPrimary_Bidi_singleLine_textDirectionLtr
#[test]
fn secondary_horizontal_positions_of_bidi_text_left_to_right() {
    let expected = [0.0, 50.0, 100.0, 300.0, 250.0, 200.0, 150.0];
    assert_eq!(horizontals(MIXED, sized(50.0), false), expected);
    assert_eq!(horizontals(MIXED, directed(50.0, TextDirection::Ltr), false), expected);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getHorizontalPositionForOffset_primary_ltr_singleLine_textDirectionRtl
#[test]
fn primary_horizontal_positions_of_left_to_right_text_in_a_right_to_left_paragraph() {
    let positions = horizontals("abc", directed(50.0, TextDirection::Rtl), true);
    assert_eq!(positions, [150.0, 50.0, 100.0, 0.0]);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getHorizontalPositionForOffset_notPrimary_ltr_singleLine_textDirectionRtl
#[test]
fn secondary_horizontal_positions_of_left_to_right_text_in_a_right_to_left_paragraph() {
    let positions = horizontals("abc", directed(50.0, TextDirection::Rtl), false);
    assert_eq!(positions, [0.0, 50.0, 100.0, 150.0]);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getHorizontalPositionForOffset_primary_rtl_singleLine_textDirectionLtr
#[test]
fn primary_horizontal_positions_of_right_to_left_text_in_a_left_to_right_paragraph() {
    let positions = horizontals(HEBREW, directed(50.0, TextDirection::Ltr), true);
    assert_eq!(positions, [0.0, 100.0, 50.0, 150.0]);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getHorizontalPositionForOffset_notPrimary_rtl_singleLine_textDirectionLtr
#[test]
fn secondary_horizontal_positions_of_right_to_left_text_in_a_left_to_right_paragraph() {
    let positions = horizontals(HEBREW, directed(50.0, TextDirection::Ltr), false);
    assert_eq!(positions, [150.0, 100.0, 50.0, 0.0]);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getHorizontalPositionForOffset_primary_Bidi_singleLine_textDirectionRtl
#[test]
fn primary_horizontal_positions_of_bidi_text_right_to_left() {
    let positions = horizontals(MIXED, directed(50.0, TextDirection::Rtl), true);
    assert_eq!(positions, [300.0, 200.0, 250.0, 150.0, 100.0, 50.0, 0.0]);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getHorizontalPositionForOffset_notPrimary_Bidi_singleLine_textDirectionRtl
#[test]
fn secondary_horizontal_positions_of_bidi_text_right_to_left() {
    let positions = horizontals(MIXED, directed(50.0, TextDirection::Rtl), false);
    assert_eq!(positions, [150.0, 200.0, 250.0, 300.0, 100.0, 50.0, 0.0]);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getHorizontalPositionForOffset_primary_ltr_newLine_textDirectionDefault
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getHorizontalPositionForOffset_primary_rtl_newLine_textDirectionDefault
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getHorizontalPositionForOffset_primary_rtl_newLine_textDirectionLtr
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getHorizontalPositionForOffset_notPrimary_ltr_newLine_textDirectionDefault
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getHorizontalPositionForOffset_notPrimary_rtl_newLine_textDirectionDefault
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getHorizontalPositionForOffset_notPrimary_rtl_newLine_textDirectionLtr
#[test]
fn the_line_after_a_final_line_feed_starts_at_the_left() {
    let rtl = "\u{05D0}\u{05D1}\u{05D2}\n";
    for (text, style) in
        [("abc\n", sized(50.0)), (rtl, sized(50.0)), (rtl, directed(50.0, TextDirection::Ltr))]
    {
        for primary in [true, false] {
            let positions = horizontals(text, style.clone(), primary);
            assert_eq!(positions[4], 0.0, "{text} {primary}");
        }
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getHorizontalPositionForOffset_primary_ltr_newLine_textDirectionRtl
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getHorizontalPositionForOffset_notPrimary_ltr_newLine_textDirectionRtl
#[test]
fn the_line_after_a_final_line_feed_in_a_right_to_left_paragraph_starts_at_the_right() {
    for primary in [true, false] {
        let positions = horizontals("abc\n", directed(50.0, TextDirection::Rtl), primary);
        assert_eq!(positions[4], 200.0, "{primary}");
    }
}

/// The paragraph directions at each character boundary of `text` laid out in `style`.
fn paragraph_directions(text: &str, style: TextStyle) -> Vec<ResolvedTextDirection> {
    let paragraph = paragraph(text, style, Some(chars(text) as f32 * 50.0));
    (0..=chars(text)).map(|i| paragraph.get_paragraph_direction(at(text, i))).collect()
}

/// The bidi run directions at each character boundary of `text` laid out in `style`.
fn run_directions(text: &str, style: TextStyle) -> Vec<ResolvedTextDirection> {
    let paragraph = paragraph(text, style, Some(chars(text) as f32 * 50.0));
    (0..=chars(text)).map(|i| paragraph.get_bidi_run_direction(at(text, i))).collect()
}

use ResolvedTextDirection::{Ltr, Rtl};

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getParagraphDirection_ltr_singleLine_textDirectionDefault
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getParagraphDirection_ltr_singleLine_textDirectionRtl
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getParagraphDirection_rtl_singleLine_textDirectionDefault
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getParagraphDirection_rtl_singleLine_textDirectionLtr
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getParagraphDirection_Bidi_singleLine_textDirectionDefault
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getParagraphDirection_Bidi_singleLine_textDirectionLtr
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getParagraphDirection_Bidi_singleLine_textDirectionRtl
#[test]
fn the_directions_of_paragraphs() {
    let rtl_line = "\u{05D0}\u{05D1}\u{05D2}\n";
    assert_eq!(paragraph_directions("abc", sized(50.0)), [Ltr; 4]);
    assert_eq!(paragraph_directions("abc", directed(50.0, TextDirection::Rtl)), [Rtl; 4]);
    assert_eq!(paragraph_directions(rtl_line, sized(50.0))[..4], [Rtl; 4]);
    assert_eq!(paragraph_directions(rtl_line, directed(50.0, TextDirection::Ltr)), [Ltr; 5]);
    assert_eq!(paragraph_directions(MIXED, sized(50.0)), [Ltr; 7]);
    assert_eq!(paragraph_directions(MIXED, directed(50.0, TextDirection::Ltr)), [Ltr; 7]);
    assert_eq!(paragraph_directions(MIXED, directed(50.0, TextDirection::Rtl)), [Rtl; 7]);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getBidiRunDirection_ltr_singleLine_textDirectionDefault
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getBidiRunDirection_ltr_singleLine_textDirectionRtl
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getBidiRunDirection_rtl_singleLine_textDirectionDefault
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getBidiRunDirection_rtl_singleLine_textDirectionLtr
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getBidiRunDirection_Bidi_singleLine_textDirectionDefault
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getBidiRunDirection_Bidi_singleLine_textDirectionLtr
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getBidiRunDirection_Bidi_singleLine_textDirectionRtl
#[test]
fn the_directions_of_bidi_runs() {
    let rtl_line = "\u{05D0}\u{05D1}\u{05D2}\n";
    assert_eq!(run_directions("abc", sized(50.0)), [Ltr; 4]);
    assert_eq!(run_directions("abc", directed(50.0, TextDirection::Rtl)), [Ltr; 4]);
    assert_eq!(run_directions(rtl_line, sized(50.0))[..4], [Rtl; 4]);
    assert_eq!(
        run_directions(rtl_line, directed(50.0, TextDirection::Ltr))[..4],
        [Rtl, Rtl, Rtl, Ltr]
    );
    for direction in [None, Some(TextDirection::Ltr), Some(TextDirection::Rtl)] {
        let style = TextStyle { text_direction: direction, ..sized(50.0) };
        assert_eq!(run_directions(MIXED, style)[..6], [Ltr, Ltr, Ltr, Rtl, Rtl, Rtl]);
    }
}

/// `text` at 100 sp with at most `max_lines` lines, unbounded.
fn limited(text: &str, max_lines: u32, overflow: TextOverflow) -> Rc<dyn Paragraph> {
    lay_out(
        text.to_string(),
        Setup { style: sized(100.0), max_lines, overflow, ..Setup::default() },
    )
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, lineCount_withMaxLineSmallerThanTextLines
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, lineCount_withMaxLineGreaterThanTextLines
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, maxLines_withMaxLineSmallerThanTextLines_clipHeight
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, maxLines_withMaxLineEqualsTextLine
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, maxLines_withMaxLineGreaterThanTextLines
#[test]
fn max_lines_cut_the_lines_and_the_height() {
    let cut = limited("a\na\na", 2, TextOverflow::Clip);
    assert_eq!(cut.line_count(), 2);
    assert_eq!(cut.height(), 200.0);
    let all = limited("a\na\na", 3, TextOverflow::Clip);
    assert_eq!(all.height(), 300.0);
    let more = limited("a\na\na", 4, TextOverflow::Clip);
    assert_eq!(more.line_count(), 3);
    assert_eq!(more.height(), 300.0);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, maxLines_withMaxLineSmallerThanTextLines_haveCorrectBaselines
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, maxLines_withMaxLineGreaterThanTextLines_haveCorrectBaselines
#[test]
fn max_lines_keep_the_baselines() {
    let cut = limited("a\na\na", 2, TextOverflow::Clip);
    assert_eq!(cut.first_baseline(), 80.0);
    assert_eq!(cut.last_baseline(), 180.0);
    let more = limited("a\na\na", 4, TextOverflow::Clip);
    assert_eq!(more.first_baseline(), 80.0);
    assert_eq!(more.last_baseline(), 280.0);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, didExceedMaxLines_withMaxLinesSmallerThanTextLines_returnsTrue
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, didExceedMaxLines_withMaxLinesEqualToTextLines_returnsFalse
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, didExceedMaxLines_withMaxLinesGreaterThanTextLines_returnsFalse
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, didExceedMaxLines_ellipsis_withMaxLinesSmallerThanTextLines_returnsTrue
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, didExceedMaxLines_ellipsis_withMaxLinesEqualToTextLines_returnsFalse
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, didExceedMaxLines_ellipsis_withMaxLinesGreaterThanTextLines_returnsFalse
#[test]
fn more_lines_than_the_maximum_exceed_it() {
    for overflow in [TextOverflow::Clip, TextOverflow::Ellipsis] {
        assert!(limited("aaa\naa", 1, overflow).did_exceed_max_lines());
        assert!(!limited("aaa\naa", 2, overflow).did_exceed_max_lines());
        assert!(!limited("aaa\naa", 3, overflow).did_exceed_max_lines());
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, didExceedMaxLines_withMaxLinesSmallerThanTextLines_withLineWrap_returnsTrue
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, didExceedMaxLines_withMaxLinesEqualToTextLines_withLineWrap_returnsFalse
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, didExceedMaxLines_withMaxLinesGreaterThanTextLines_withLineWrap_returnsFalse
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, didExceedMaxLines_ellipsis_withMaxLinesSmallerThanTextLines_withLineWrap_returnsTrue
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, didExceedMaxLines_ellipsis_withMaxLinesEqualToTextLines_withLineWrap_returnsFalse
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, didExceedMaxLines_ellipsis_withMaxLinesGreaterThanTextLines_withLineWrap_returnsFalse
#[test]
fn wrapped_lines_past_the_maximum_exceed_it() {
    for overflow in [TextOverflow::Clip, TextOverflow::Ellipsis] {
        let wrapped = |text: &str, max_lines| {
            let setup = Setup {
                style: sized(50.0),
                width: Some(50.0),
                max_lines,
                overflow,
                ..Setup::default()
            };
            lay_out(text.to_string(), setup).did_exceed_max_lines()
        };
        assert!(wrapped("aa", 1));
        assert!(!wrapped("a", 1));
        assert!(!wrapped("aa", 3));
    }
}

/// The paragraph of `text` at 20 sp aligned `align`, two characters wider than the text.
fn aligned(text: &str, align: Option<TextAlign>) -> (Rc<dyn Paragraph>, f32) {
    let width = (chars(text) + 2) as f32 * 20.0;
    let style = TextStyle { text_align: align, ..sized(20.0) };
    (paragraph(text, style, Some(width)), width)
}

const HEBREW_PAIR: &str = "\u{05D0}\u{05D0}";

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, textAlign_defaultValue_alignsStart
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, textAlign_whenAlignStart_withLTR_returnsZeroForGetLineLeft
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, textAlign_whenAlignStart_withRTL_returnsLayoutWidthForGetLineRight
#[test]
fn text_aligns_to_its_start() {
    for align in [None, Some(TextAlign::Start)] {
        assert_eq!(aligned("aa", align).0.get_line_left(0), 0.0);
        let (paragraph, width) = aligned(HEBREW_PAIR, align);
        assert_eq!(paragraph.get_line_right(0), width);
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, textAlign_whenAlignLeft_returnsZeroForGetLineLeft
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, textAlign_whenAlignRight_returnsLayoutWidthForGetLineRight
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, textAlign_whenAlignCenter_textIsCentered
#[test]
fn text_aligns_left_right_or_center_whatever_its_direction() {
    for text in ["aa", HEBREW_PAIR] {
        assert_eq!(aligned(text, Some(TextAlign::Left)).0.get_line_left(0), 0.0, "{text}");
        let (right, width) = aligned(text, Some(TextAlign::Right));
        assert_eq!(right.get_line_right(0), width, "{text}");
        let (center, width) = aligned(text, Some(TextAlign::Center));
        assert_eq!(center.get_line_left(0), width / 2.0 - 20.0, "{text}");
        assert_eq!(center.get_line_right(0), width / 2.0 + 20.0, "{text}");
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, textAlign_whenAlignEnd_withLTR_returnsLayoutWidthForGetLineRight
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, textAlign_whenAlignEnd_withRTL_returnsZeroForGetLineLeft
#[test]
fn text_aligns_to_its_end() {
    let (paragraph, width) = aligned("aa", Some(TextAlign::End));
    assert_eq!(paragraph.get_line_right(0), width);
    assert_eq!(aligned(HEBREW_PAIR, Some(TextAlign::End)).0.get_line_left(0), 0.0);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, textAlign_whenAlignJustify_justifies
#[test]
fn justified_text_fills_its_lines_but_the_last() {
    let style = TextStyle { text_align: Some(TextAlign::Justify), ..sized(20.0) };
    let paragraph = paragraph("a a a", style, Some(80.0));
    assert_eq!(paragraph.get_line_left(0), 0.0);
    assert_eq!(paragraph.get_line_right(0), 80.0);
    assert_eq!(paragraph.get_line_left(1), 0.0);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, textDirection_whenLTR_dotIsOnRight
#[test]
fn a_left_to_right_paragraph_ends_with_its_dots_on_the_right() {
    let paragraph = paragraph("a..", directed(20.0, TextDirection::Ltr), Some(60.0));
    assert_eq!(paragraph.get_offset_for_position(Offset::new(41.0, 10.0)), 2);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, textDirection_whenRTL_dotIsOnLeft
#[test]
fn a_right_to_left_paragraph_ends_with_its_dots_on_the_left() {
    let paragraph = paragraph("a..", directed(20.0, TextDirection::Rtl), Some(60.0));
    assert_eq!(paragraph.get_offset_for_position(Offset::new(11.0, 10.0)), 2);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, textDirection_whenDefault_withoutStrongChar_directionIsLTR
#[test]
fn a_paragraph_without_a_strong_character_runs_left_to_right() {
    let paragraph = paragraph("...", sized(20.0), Some(60.0));
    for i in 0..=3 {
        let position = Offset::new(i as f32 * 20.0 + 1.0, 10.0);
        assert_eq!(paragraph.get_offset_for_position(position), i);
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, textDirection_whenDefault_withFirstStrongCharLTR_directionIsLTR
#[test]
fn a_paragraph_runs_as_its_first_strong_character_left_to_right() {
    let text = "a\u{05D0}.";
    let paragraph = paragraph(text, sized(20.0), Some(60.0));
    for i in 0..3 {
        let position = Offset::new(i as f32 * 20.0 + 1.0, 10.0);
        assert_eq!(paragraph.get_offset_for_position(position), at(text, i));
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, textDirection_whenDefault_withFirstStrongCharRTL_directionIsRTL
#[test]
fn a_paragraph_runs_as_its_first_strong_character_right_to_left() {
    let text = "\u{05D0}a.";
    let paragraph = paragraph(text, sized(20.0), Some(60.0));
    assert_eq!(paragraph.get_offset_for_position(Offset::new(11.0, 10.0)), at(text, 2));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getLineTop
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getLineBottom
#[test]
fn line_tops_and_bottoms() {
    let paragraph = paragraph("aaa\nbbb", sized(50.0), None);
    assert_eq!(paragraph.get_line_top(0), 0.0);
    assert_eq!(paragraph.get_line_top(1), 50.0);
    assert_eq!(paragraph.get_line_bottom(0), 50.0);
    assert_eq!(paragraph.get_line_bottom(1), 100.0);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getLineBaseline
#[test]
fn line_baselines() {
    let paragraph = paragraph("aaa\nbbb\nccc", sized(50.0), None);
    assert_eq!(paragraph.get_line_baseline(0), 40.0);
    assert_eq!(paragraph.get_line_baseline(0), paragraph.first_baseline());
    assert_eq!(paragraph.get_line_baseline(1), 90.0);
    assert_eq!(paragraph.get_line_baseline(2), 140.0);
    assert_eq!(paragraph.get_line_baseline(2), paragraph.last_baseline());
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getLineForOffset_withNewline
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getLineForOffset_newline_belongsToPreviousLine
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getLineForOffset_outOfBoundary
#[test]
fn the_line_for_an_offset() {
    let two_lines = paragraph("aaa\nbbb", TextStyle::DEFAULT, None);
    for i in 0..=2 {
        assert_eq!(two_lines.get_line_for_offset(i), 0);
    }
    for i in 4..=6 {
        assert_eq!(two_lines.get_line_for_offset(i), 1);
    }
    assert_eq!(two_lines.get_line_for_offset(7), 1);
    assert_eq!(two_lines.get_line_for_offset(8), 1);
    let fed = paragraph("aaa\nbbb\n", TextStyle::DEFAULT, None);
    assert_eq!(fed.get_line_for_offset(3), 0);
    assert_eq!(fed.get_line_for_offset(7), 1);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getLineForOffset_ellipsisApplied
#[test]
fn an_ellipsized_line_holds_the_rest_of_the_text() {
    let setup = Setup { max_lines: 1, overflow: TextOverflow::Ellipsis, ..Setup::default() };
    let paragraph = lay_out("aaa\nbbb", setup);
    for i in 0..=8 {
        assert_eq!(paragraph.get_line_for_offset(i), 0, "at {i}");
    }
    assert_eq!(paragraph.get_line_start(0), 0);
    assert_eq!(paragraph.get_line_end(0, false), 7);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getLineStart_linebreak
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getLineEnd_linebreak
#[test]
fn line_starts_at_a_line_break() {
    let paragraph = paragraph("aaabbb", sized(50.0), Some(150.0));
    assert_eq!(paragraph.line_count(), 2);
    assert_eq!(paragraph.get_line_start(0), 0);
    assert_eq!(paragraph.get_line_start(1), 3);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getLineStart_newline
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getLineEnd_newline
#[test]
fn line_starts_and_ends_at_a_line_feed() {
    let paragraph = paragraph("aaa\nbbb", TextStyle::DEFAULT, None);
    assert_eq!(paragraph.line_count(), 2);
    assert_eq!(paragraph.get_line_start(0), 0);
    assert_eq!(paragraph.get_line_start(1), 4);
    assert_eq!(paragraph.get_line_end(0, false), 4);
    assert_eq!(paragraph.get_line_end(1, false), 7);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getLineStart_emptyLine
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getLineEnd_emptyLine
#[test]
fn an_empty_last_line_starts_and_ends_at_the_texts_end() {
    let paragraph = paragraph("aaa\n", TextStyle::DEFAULT, None);
    assert_eq!(paragraph.line_count(), 2);
    assert_eq!(paragraph.get_line_start(0), 0);
    assert_eq!(paragraph.get_line_start(1), 4);
    assert_eq!(paragraph.get_line_end(0, false), 4);
    assert_eq!(paragraph.get_line_end(1, false), 4);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getLineEllipsisOffset
#[test]
fn an_ellipsized_last_line_ends_at_the_texts_end_and_shows_its_own() {
    let setup = Setup { max_lines: 2, overflow: TextOverflow::Ellipsis, ..Setup::default() };
    let paragraph = lay_out("aaa\nbbb\nccc", setup);
    assert_eq!(paragraph.line_count(), 2);
    assert_eq!(paragraph.get_line_end(0, false), 4);
    assert_eq!(paragraph.get_line_end(0, true), 3);
    assert!(!paragraph.is_line_ellipsized(0));
    assert_eq!(paragraph.get_line_end(1, false), 11);
    assert_eq!(paragraph.get_line_end(1, true), 7);
    assert!(paragraph.is_line_ellipsized(1));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getLineEllipsisCount
#[test]
fn a_wrapped_ellipsized_line_ends_at_the_texts_end() {
    let setup = Setup {
        style: sized(10.0),
        width: Some(50.0),
        max_lines: 2,
        overflow: TextOverflow::Ellipsis,
        ..Setup::default()
    };
    let paragraph = lay_out("aaaaabbbbbccccc", setup);
    assert_eq!(paragraph.line_count(), 2);
    assert!(!paragraph.is_line_ellipsized(0));
    assert_eq!(paragraph.get_line_start(0), 0);
    assert_eq!(paragraph.get_line_end(0, false), 5);
    assert_eq!(paragraph.get_line_end(0, true), 5);
    assert!(paragraph.is_line_ellipsized(1));
    assert_eq!(paragraph.get_line_start(1), 5);
    assert_eq!(paragraph.get_line_end(1, false), 15);
    assert_ne!(paragraph.get_line_end(1, true), 15);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, lineHeight_inSp
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, lineHeight_InEm
#[test]
fn a_line_height_sets_the_lines_between_the_first_and_the_last() {
    for line_height in [30.sp(), 1.5.em()] {
        let style = TextStyle { line_height, ..sized(20.0) };
        let paragraph = paragraph("abcdefgh", style, Some(40.0));
        assert_eq!(paragraph.line_count(), 4);
        for line in 1..3 {
            assert_eq!(paragraph.get_line_height(line), 30.0, "line {line}");
        }
    }
}

/// `text` in `style` with `spans` over it, unbounded.
fn spanned(text: &str, style: TextStyle, spans: &[(SpanStyle, usize, usize)]) -> Rc<dyn Paragraph> {
    let mut builder = compose_ui::AnnotatedStringBuilder::new();
    builder.append(text);
    for (span, start, end) in spans {
        builder.add_style(span.clone(), *start, *end);
    }
    lay_out(builder.to_annotated_string(), Setup { style, ..Setup::default() })
}

fn span_size(size: compose_ui::TextUnit) -> SpanStyle {
    SpanStyle { font_size: size, ..SpanStyle::DEFAULT }
}

fn span_spacing(spacing: compose_ui::TextUnit) -> SpanStyle {
    SpanStyle { letter_spacing: spacing, ..SpanStyle::DEFAULT }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testAnnotatedString_setFontSizeOnWholeText
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testAnnotatedString_setFontSizeOnPartOfText
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testAnnotatedString_seFontSizeTwice_lastOneOverwrite
#[test]
fn span_font_sizes_size_their_characters() {
    let whole = spanned("abcde", TextStyle::DEFAULT, &[(span_size(20.sp()), 0, 5)]);
    assert_eq!(whole.line_count(), 1);
    assert_eq!(whole.get_line_width(0), 100.0);
    let part = spanned("abcde", sized(20.0), &[(span_size(30.sp()), 0, 3)]);
    assert_eq!(part.get_line_width(0), 3.0 * 30.0 + 2.0 * 20.0);
    let twice = spanned(
        "abcde",
        TextStyle::DEFAULT,
        &[(span_size(20.sp()), 0, 5), (span_size(30.sp()), 0, 3)],
    );
    assert_eq!(twice.get_line_width(0), 3.0 * 30.0 + 2.0 * 20.0);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testAnnotatedString_fontSizeScale
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testAnnotatedString_fontSizeScaleNested
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testAnnotatedString_fontSizeScaleWithFontSizeFirst
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testAnnotatedString_fontSizeScaleWithFontSizeSecond
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testAnnotatedString_fontSizeScaleWithFontSizeNested
#[test]
fn span_font_sizes_in_ems_scale_the_size_they_cover() {
    let style = sized(20.0);
    let right = |spans: &[(SpanStyle, usize, usize)]| {
        spanned("abcde", style.clone(), spans).get_line_right(0)
    };
    assert_eq!(right(&[(span_size(0.5.em()), 0, 5)]), 5.0 * 10.0);
    assert_eq!(right(&[(span_size(0.5.em()), 0, 5), (span_size(2.0.em()), 0, 5)]), 5.0 * 20.0);
    assert_eq!(right(&[(span_size(30.sp()), 0, 5), (span_size(0.5.em()), 0, 5)]), 5.0 * 15.0);
    assert_eq!(right(&[(span_size(0.5.em()), 0, 5), (span_size(30.sp()), 0, 5)]), 5.0 * 30.0);
    assert_eq!(
        right(&[
            (span_size(0.5.em()), 0, 5),
            (span_size(30.sp()), 0, 5),
            (span_size(2.0.em()), 0, 5),
        ]),
        5.0 * 60.0
    );
}

// Compose's expectations without the platform's fix for the letter spacing at a line's edges
// (each character takes its full spacing, as the fixed measurer lays it out).
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testAnnotatedString_setLetterSpacing_inEm_OnWholeText
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testAnnotatedString_setLetterSpacing_inSp_OnWholeText
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testAnnotatedString_setLetterSpacingOnPartText
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testAnnotatedString_setLetterSpacingTwice_lastOneOverwrite
#[test]
fn span_letter_spacings_widen_their_characters() {
    let style = sized(20.0);
    let width = |spans: &[(SpanStyle, usize, usize)]| {
        spanned("abcde", style.clone(), spans).get_line_width(0)
    };
    assert_eq!(width(&[(span_spacing(5.0.em()), 0, 5)]), 20.0 * 5.0 * 6.0);
    assert_eq!(width(&[(span_spacing(5.sp()), 0, 5)]), 25.0 * 5.0);
    assert_eq!(width(&[(span_spacing(5.0.em()), 0, 3)]), (3.0 * 5.0 + 5.0) * 20.0);
    assert_eq!(
        width(&[(span_spacing(5.0.em()), 0, 5), (span_spacing(10.0.em()), 0, 3)]),
        3.0 * 11.0 * 20.0 + 2.0 * 6.0 * 20.0
    );
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testAnnotatedString_setLetterSpacing_inEm_withFontSize
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testAnnotatedString_setLetterSpacing_inEm_withScaleX
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testAnnotatedString_setLetterSpacing_inSp_withFontSize
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testAnnotatedString_setLetterSpacing_inSp_withScaleX
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testAnnotatedString_setLetterSpacing_inSp_after_inEm
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testAnnotatedString_setLetterSpacing_inEm_after_inSp
#[test]
fn letter_spacings_in_ems_follow_the_size_and_scale_of_their_characters() {
    let style = sized(20.0);
    let width = |spans: &[(SpanStyle, usize, usize)]| {
        spanned("abcde", style.clone(), spans).get_line_width(0)
    };
    let scaled = SpanStyle {
        text_geometric_transform: Some(TextGeometricTransform { scale_x: 1.5, skew_x: 0.0 }),
        ..SpanStyle::DEFAULT
    };
    assert_eq!(
        width(&[(span_spacing(2.0.em()), 0, 5), (span_size(30.sp()), 0, 3)]),
        3.0 * (3.0 * 30.0 + 2.0 * 20.0)
    );
    assert_eq!(
        width(&[(span_spacing(2.0.em()), 0, 5), (scaled.clone(), 0, 3)]),
        3.0 * (3.0 * 20.0 * 1.5 + 2.0 * 20.0)
    );
    assert_eq!(
        width(&[(span_spacing(10.sp()), 0, 5), (span_size(30.sp()), 0, 3)]),
        5.0 * 10.0 + 3.0 * 30.0 + 2.0 * 20.0
    );
    assert_eq!(
        width(&[(span_spacing(10.sp()), 0, 5), (scaled, 0, 3)]),
        5.0 * 10.0 + 3.0 * 20.0 * 1.5 + 2.0 * 20.0
    );
    assert_eq!(
        width(&[(span_spacing(1.0.em()), 0, 5), (span_spacing(10.sp()), 0, 3)]),
        20.0 * 5.0 + 3.0 * 10.0 + 2.0 * 20.0
    );
    assert_eq!(
        width(&[(span_spacing(10.sp()), 0, 3), (span_spacing(1.0.em()), 0, 5)]),
        20.0 * 5.0 * 2.0
    );
}

fn indented(first_line: TextUnit, rest_line: TextUnit) -> TextStyle {
    TextStyle { text_indent: Some(TextIndent { first_line, rest_line }), ..sized(20.0) }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, textIndent_inSp_onSingleLine
#[test]
fn a_first_line_indent_moves_the_first_line() {
    let paragraph = paragraph("abc", indented(20.sp(), 0.sp()), None);
    assert_eq!(paragraph.get_offset_for_position(Offset::new(21.0, 10.0)), 0);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, textIndent_inSp_onFirstLine
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, textIndent_inEm_onSingleLine
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, textIndent_inEm_onFirstLine
#[test]
fn a_first_line_indent_starts_the_text_after_it() {
    let first = paragraph("abcdef", indented(15.sp(), 0.sp()), Some(80.0));
    assert_eq!(first.line_count(), 2);
    assert_eq!(first.get_horizontal_position(0, true), 15.0);
    let single = paragraph("abc", indented(1.5.em(), 0.sp()), None);
    assert_eq!(single.get_horizontal_position(0, true), 30.0);
    let wrapped = paragraph("abcdef", indented(1.5.em(), 0.sp()), Some(80.0));
    assert_eq!(wrapped.line_count(), 2);
    assert_eq!(wrapped.get_horizontal_position(0, true), 30.0);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, textIndent_inSp_onRestLine
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, textIndent_inEm_onRestLine
#[test]
fn a_rest_line_indent_starts_the_other_lines_after_it() {
    let sp = paragraph("abcde", indented(0.sp(), 20.sp()), Some(60.0));
    assert_eq!(sp.get_horizontal_position(3, true), 20.0);
    let em = paragraph("abcdef", indented(0.sp(), 1.5.em()), Some(80.0));
    assert_eq!(em.line_count(), 2);
    assert_eq!(em.get_horizontal_position(4, true), 30.0);
}

/// The rectangles added to `path` (each contour's bounds).
fn path_rects(path: &compose_ui::Path) -> Vec<Rect> {
    use compose_ui::PathCommand;
    let data = path.data();
    let mut rects = Vec::new();
    let mut points: Vec<Offset> = Vec::new();
    let mut flush = |points: &mut Vec<Offset>| {
        if let Some(first) = points.first() {
            let mut rect = Rect::new(first.x, first.y, first.x, first.y);
            for point in points.iter() {
                rect = Rect::new(
                    rect.left.min(point.x),
                    rect.top.min(point.y),
                    rect.right.max(point.x),
                    rect.bottom.max(point.y),
                );
            }
            rects.push(rect);
        }
        points.clear();
    };
    for command in data.commands() {
        match *command {
            PathCommand::MoveTo(point) => {
                flush(&mut points);
                points.push(point);
            }
            PathCommand::LineTo(point) => points.push(point),
            _ => {}
        }
    }
    flush(&mut points);
    rects
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testGetPathForRange_singleLine
#[test]
fn the_path_of_a_range_on_one_line() {
    let paragraph = paragraph("abc", sized(20.0), None);
    let path = paragraph.get_path_for_range(0, 2);
    assert_eq!(path_rects(&path), [Rect::new(0.0, 0.0, 40.0, 20.0)]);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testGetPathForRange_multiLines
#[test]
fn the_path_of_a_range_on_two_lines() {
    let paragraph = paragraph("abc\nabc", sized(20.0), None);
    let rects = path_rects(&paragraph.get_path_for_range(1, 6));
    // "bc" on the first line, "ab" on the second (Android's path also extends the first line
    // to the paragraph's end and the last one from its start).
    assert!(rects.contains(&Rect::new(20.0, 0.0, 60.0, 20.0)), "{rects:?}");
    assert!(rects.contains(&Rect::new(0.0, 20.0, 40.0, 40.0)), "{rects:?}");
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testGetPathForRange_Bidi
#[test]
fn the_path_of_a_range_across_bidi_runs() {
    let text = "Hello\u{05E9}\u{05DC}\u{05D5}\u{05DD}";
    let paragraph = paragraph(text, sized(20.0), None);
    let rects = path_rects(&paragraph.get_path_for_range(2, at(text, 7)));
    let (left, right) = (paragraph.get_line_left(0), paragraph.get_line_right(0));
    assert!(rects.contains(&Rect::new(left + 40.0, 0.0, left + 100.0, 20.0)), "{rects:?}");
    assert!(rects.contains(&Rect::new(right - 40.0, 0.0, right, 20.0)), "{rects:?}");
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testGetPathForRange_Start_Equals_End_Returns_Empty_Path
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testGetPathForRange_Empty_Text
#[test]
fn the_path_of_an_empty_range_is_empty() {
    assert!(paragraph("abc", sized(20.0), None).get_path_for_range(1, 1).is_empty());
    assert!(paragraph("", sized(20.0), None).get_path_for_range(0, 0).is_empty());
}

const CLEFS: &str = "\u{1D11E}\u{1D11F}";

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testGetPathForRange_Surrogate_Pair_Start_Middle_Second_Character_Selected
#[test]
fn a_range_starting_inside_a_character_starts_after_it() {
    let paragraph = paragraph(CLEFS, sized(20.0), None);
    let path = paragraph.get_path_for_range(1, CLEFS.len());
    assert_eq!(path_rects(&path), [Rect::new(20.0, 0.0, 40.0, 20.0)]);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testGetPathForRange_Surrogate_Pair_End_Middle_Second_Character_Selected
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testGetPathForRange_Surrogate_Pair_Start_Middle_End_Same_Character_Returns_Line_Segment
#[test]
fn a_range_inside_characters_covers_none_of_them() {
    let paragraph = paragraph(CLEFS, sized(20.0), None);
    assert!(paragraph.get_path_for_range(1, CLEFS.len() - 1).is_empty());
    assert!(paragraph.get_path_for_range(1, 2).is_empty());
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testGetPathForRange_Emoji_Sequence
#[test]
fn the_path_of_a_range_of_emoji() {
    let text = "\u{1F600}\u{1F603}\u{1F604}\u{1F606}";
    let paragraph = paragraph(text, sized(20.0), None);
    let path = paragraph.get_path_for_range(1, text.len() - 1);
    assert_eq!(path_rects(&path), [Rect::new(20.0, 0.0, 60.0, 20.0)]);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testGetPathForRange_Unicode_200D_Return_Line_Segment
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testGetPathForRange_Unicode_2066_Return_Line_Segment
#[test]
fn invisible_characters_take_no_width() {
    for text in ["\u{200D}", "\u{2066}"] {
        let paragraph = paragraph(text, sized(20.0), None);
        assert_eq!(paragraph.get_line_left(0), paragraph.get_line_right(0), "{text:?}");
        for rect in path_rects(&paragraph.get_path_for_range(0, text.len())) {
            assert_eq!(rect.left, rect.right, "{text:?}");
        }
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getPathForRange_throws_exception_if_start_larger_than_end
#[test]
#[should_panic(expected = "out of range")]
fn a_reversed_path_range_panics() {
    paragraph("ab", TextStyle::DEFAULT, None).get_path_for_range(2, 0);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getPathForRange_throws_exception_if_end_is_larger_than_text_length
#[test]
#[should_panic(expected = "out of range")]
fn a_path_range_past_the_text_panics() {
    paragraph("ab", TextStyle::DEFAULT, None).get_path_for_range(0, 3);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testGetWordBoundary
#[test]
fn the_word_at_an_offset() {
    let paragraph = paragraph("abc def", sized(20.0), None);
    assert_eq!(paragraph.get_word_boundary(0), TextRange::new(0, 3));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testGetWordBoundary_spaces
#[test]
fn the_word_at_an_offset_around_spaces() {
    let paragraph = paragraph("ab cd  e", sized(20.0), None);
    // A word's end selects it, as its start does.
    assert_eq!(paragraph.get_word_boundary(2), TextRange::new(0, 2));
    assert_eq!(paragraph.get_word_boundary(3), TextRange::new(3, 5));
    // Between spaces: the offset itself.
    assert_eq!(paragraph.get_word_boundary(6), TextRange::new(6, 6));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testGetWordBoundary_Bidi
#[test]
fn the_word_at_an_offset_in_bidi_text() {
    let text = "abc \u{05D0}\u{05D1}\u{05D2} def";
    let paragraph = paragraph(text, sized(20.0), None);
    assert_eq!(paragraph.get_word_boundary(0), TextRange::new(0, 3));
    assert_eq!(paragraph.get_word_boundary(at(text, 5)), TextRange::new(at(text, 4), at(text, 7)));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getWordBoundary_emoji
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getWordBoundary_letters_emojis_mixed
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getWordBoundary_multiple_emojis
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getWordBoundary_multichar
#[test]
fn words_of_emoji_and_of_characters_outside_the_basic_plane() {
    let person = "\u{1F9D1}\u{1F3FF}\u{200D}\u{1F9B0}";
    let text = format!("ab {person} cd");
    let words = paragraph(&text, TextStyle::DEFAULT, None);
    assert_eq!(words.get_word_boundary(7), TextRange::new(3, 3 + person.len()));
    let mixed = format!("a b{person}c\u{1F44D}\u{1F3FE} d");
    let words = paragraph(&mixed, TextStyle::DEFAULT, None);
    let (b, d) = (mixed.find('b').unwrap(), mixed.find('d').unwrap());
    assert_eq!(words.get_word_boundary(b + 5), TextRange::new(b, d - 1));
    assert_eq!(words.get_word_boundary(mixed.len() - 4), TextRange::new(b, d - 1));
    let grins = "\u{1F600}\u{1F600}\u{1F600}";
    let words = paragraph(grins, TextStyle::DEFAULT, None);
    assert_eq!(words.get_word_boundary(4), TextRange::new(0, grins.len()));
    let deseret = "\u{10414}\u{1042F}\u{10445}\u{10428}\u{10449}\u{1042F}\u{1043B}";
    let text = format!("ab {deseret} cd");
    let words = paragraph(&text, TextStyle::DEFAULT, None);
    assert_eq!(words.get_word_boundary(7), TextRange::new(3, 3 + deseret.len()));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, test_finalFontSizeChangesWithDensity
#[test]
fn the_font_scale_scales_the_text() {
    let normal = paragraph("a", sized(20.0), None);
    let density = Density { density: 1.0, font_scale: 2.0 };
    let doubled = lay_out("a", Setup { style: sized(20.0), density, ..Setup::default() });
    assert!(doubled.max_intrinsic_width() > normal.max_intrinsic_width());
    assert!(doubled.max_intrinsic_width() <= normal.max_intrinsic_width() * 2.0);
    assert!(doubled.height() > normal.height());
    assert!(doubled.height() <= normal.height() * 2.0);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, minInstrinsicWidth_includes_white_space
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, minInstrinsicWidth_returns_longest_word_width
#[test]
fn the_min_intrinsic_width_is_the_widest_word_with_its_space() {
    assert_eq!(paragraph("b ", sized(12.0), None).min_intrinsic_width(), 24.0);
    let text: String = (1..=50).map(|length| "a".repeat(length) + " ").collect();
    assert_eq!(paragraph(&text, sized(12.0), None).min_intrinsic_width(), 51.0 * 12.0);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, minInstrinsicWidth_withStyledText
#[test]
fn the_min_intrinsic_width_follows_span_sizes() {
    let paragraph = spanned("a bb ccc", sized(12.0), &[(span_size(24.sp()), 1, 5)]);
    assert_eq!(paragraph.min_intrinsic_width(), 3.0 * 24.0);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, createParagraph_with_ParagraphIntrinsics
#[test]
fn a_paragraph_at_its_own_width_is_as_wide_as_its_text() {
    let paragraph = paragraph("abc", sized(14.0), Some(42.0));
    assert_eq!(paragraph.max_intrinsic_width(), 42.0);
    assert_eq!(paragraph.width(), 42.0);
}

// Without the platform's edge fix, as the letter spacing tests above.
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testDefaultSpanStyle_setLetterSpacing
#[test]
fn the_styles_letter_spacing_widens_each_character() {
    let style = TextStyle { letter_spacing: 1.0.em(), ..sized(100.0) };
    let paragraph = paragraph("abc", style, None);
    assert_eq!(paragraph.get_line_right(0), 100.0 * 2.0 * 3.0);
}

/// Compose's `ParagraphIntegrationBoundingBoxTest` texts: no line feed, one after the first
/// word, one after the second, left to right or right to left.
fn bounding_box_text(rtl: bool, feed: usize) -> &'static str {
    match (rtl, feed) {
        (false, 0) => "abc def abc",
        (false, 1) => "abc\ndef abc",
        (false, _) => "abc def\nabc",
        (true, 0) => "\u{05D0}\u{05D2}\u{05D2} \u{05D3}\u{05D4}\u{05D5} \u{05D0}\u{05D2}\u{05D2}",
        (true, 1) => "\u{05D0}\u{05D2}\u{05D2}\n\u{05D3}\u{05D4}\u{05D5} \u{05D0}\u{05D2}\u{05D2}",
        (true, _) => "\u{05D0}\u{05D2}\u{05D2} \u{05D3}\u{05D4}\u{05D5}\n\u{05D0}\u{05D2}\u{05D2}",
    }
}

/// The box of character `index` of a bounding box text at 10 sp, `chars` characters wide,
/// ellipsized after `max_lines`.
fn box_of(rtl: bool, feed: usize, chars: u32, max_lines: u32, index: usize) -> Rect {
    let text = bounding_box_text(rtl, feed);
    let direction = if rtl { TextDirection::Rtl } else { TextDirection::Ltr };
    let setup = Setup {
        style: directed(10.0, direction),
        width: Some(chars as f32 * 10.0),
        max_lines,
        overflow: TextOverflow::Ellipsis,
        ..Setup::default()
    };
    lay_out(text, setup).get_bounding_box(at(text, index))
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationBoundingBoxTest.kt, ltr_noBreak_noMaxLines_smallWidth_noHeight
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationBoundingBoxTest.kt, ltr_noBreak_noMaxLines_largeWidth_noHeight
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationBoundingBoxTest.kt, ltr_noBreak_1MaxLines_smallWidth_noHeight
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationBoundingBoxTest.kt, ltr_firstBreak_noMaxLines_smallWidth_noHeight
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationBoundingBoxTest.kt, ltr_firstBreak_noMaxLines_largeWidth_noHeight
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationBoundingBoxTest.kt, ltr_firstBreak_1MaxLines_smallWidth_noHeight
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationBoundingBoxTest.kt, ltr_secondBreak_noMaxLines_largeWidth_noHeight
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationBoundingBoxTest.kt, ltr_secondBreak_1MaxLines_smallWidth_noHeight
#[test]
fn bounding_boxes_of_wrapped_feed_and_ellipsized_left_to_right_text() {
    let all = u32::MAX;
    assert_eq!(box_of(false, 0, 3, all, 5), Rect::new(10.0, 10.0, 20.0, 20.0));
    assert_eq!(box_of(false, 0, 7, all, 5), Rect::new(50.0, 0.0, 60.0, 10.0));
    assert_eq!(box_of(false, 0, 3, 1, 5), Rect::new(30.0, 0.0, 30.0, 10.0));
    assert_eq!(box_of(false, 1, 3, all, 5), Rect::new(10.0, 10.0, 20.0, 20.0));
    assert_eq!(box_of(false, 1, 7, all, 5), Rect::new(10.0, 10.0, 20.0, 20.0));
    assert_eq!(box_of(false, 1, 3, 1, 5), Rect::new(30.0, 0.0, 30.0, 10.0));
    assert_eq!(box_of(false, 2, 7, all, 9), Rect::new(10.0, 10.0, 20.0, 20.0));
    assert_eq!(box_of(false, 2, 3, 1, 9), Rect::new(30.0, 0.0, 30.0, 10.0));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationBoundingBoxTest.kt, rtl_noBreak_noMaxLines_smallWidth_noHeight
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationBoundingBoxTest.kt, rtl_noBreak_noMaxLines_largeWidth_noHeight
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationBoundingBoxTest.kt, rtl_noBreak_1MaxLines_smallWidth_noHeight
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationBoundingBoxTest.kt, rtl_firstBreak_noMaxLines_smallWidth_noHeight
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationBoundingBoxTest.kt, rtl_firstBreak_noMaxLines_largeWidth_noHeight
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationBoundingBoxTest.kt, rtl_firstBreak_1MaxLines_smallWidth_noHeight
#[test]
fn bounding_boxes_of_wrapped_feed_and_ellipsized_right_to_left_text() {
    let all = u32::MAX;
    assert_eq!(box_of(true, 0, 3, all, 5), Rect::new(10.0, 10.0, 20.0, 20.0));
    assert_eq!(box_of(true, 0, 7, all, 5), Rect::new(10.0, 0.0, 20.0, 10.0));
    assert_eq!(box_of(true, 0, 3, 1, 5), Rect::new(0.0, 0.0, 0.0, 10.0));
    assert_eq!(box_of(true, 1, 3, all, 5), Rect::new(10.0, 10.0, 20.0, 20.0));
    assert_eq!(box_of(true, 1, 7, all, 5), Rect::new(50.0, 10.0, 60.0, 20.0));
    assert_eq!(box_of(true, 1, 3, 1, 5), Rect::new(0.0, 0.0, 0.0, 10.0));
}

/// The visible end of `line` of `text` at 20 sp, 10 characters wide.
fn visible_end(text: &str, max_lines: u32, overflow: TextOverflow, line: usize) -> usize {
    let setup =
        Setup { style: sized(20.0), width: Some(200.0), max_lines, overflow, ..Setup::default() };
    lay_out(text, setup).get_line_end(line, true)
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/TextLayoutLineVisibleEndTest.kt, excludesLineBreak_whenMaxLinesPresent_withoutEllipsis
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/TextLayoutLineVisibleEndTest.kt, excludesLineBreak_whenMaxLinesPresent_withEllipsisEnd
#[test]
fn a_lines_visible_end_leaves_its_line_feed_out() {
    assert_eq!(visible_end("abc\ndef", 1, TextOverflow::Clip, 0), 3);
    assert_eq!(visible_end("abc\ndef", 1, TextOverflow::Ellipsis, 0), 3);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/TextLayoutLineVisibleEndTest.kt, excludesWhitespace_singleLineContent_withEllipsis
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/TextLayoutLineVisibleEndTest.kt, excludesWhitespace_multiLineContent_withoutEllipsis
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/TextLayoutLineVisibleEndTest.kt, excludesWhitespace_multiLineContent_withEllipsis
#[test]
fn a_lines_visible_end_leaves_its_trailing_spaces_out() {
    assert_eq!(visible_end("abc def ghi", u32::MAX, TextOverflow::Ellipsis, 0), 7);
    assert_eq!(visible_end("abc def    \nghi", u32::MAX, TextOverflow::Clip, 0), 7);
    assert_eq!(visible_end("abc def    \nghi", u32::MAX, TextOverflow::Ellipsis, 0), 7);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/TextLayoutLineVisibleEndTest.kt, noVisibleContent_multiLine_withoutEllipsis
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/TextLayoutLineVisibleEndTest.kt, noVisibleContent_multiLine_withEllipsis
#[test]
fn an_empty_lines_visible_end_is_its_start() {
    for overflow in [TextOverflow::Clip, TextOverflow::Ellipsis] {
        assert_eq!(visible_end("\n\nabc", 2, overflow, 0), 0);
        assert_eq!(visible_end("\n\nabc", 2, overflow, 1), 1);
    }
}

/// The ellipsized lines of 100 characters at 10 sp, 10 a line, at most 5 lines.
fn ellipsized_lines(character: char, overflow: TextOverflow) -> Vec<bool> {
    let text: String = std::iter::repeat_n(character, 100).collect();
    let setup = Setup {
        style: sized(10.0),
        width: Some(100.0),
        max_lines: 5,
        overflow,
        ..Setup::default()
    };
    let paragraph = lay_out(text, setup);
    (0..paragraph.line_count()).map(|line| paragraph.is_line_ellipsized(line)).collect()
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/TextLayoutIsLineEllipsizedTest.kt, notEllipsized_ltr
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/TextLayoutIsLineEllipsizedTest.kt, notEllipsized_rtl
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/TextLayoutIsLineEllipsizedTest.kt, ellipsizeEnd_end_ltr
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/TextLayoutIsLineEllipsizedTest.kt, ellipsizeEnd_end_rtl
#[test]
fn only_the_last_line_of_an_ellipsized_text_is_ellipsized() {
    for character in ['a', '\u{05D0}'] {
        assert_eq!(ellipsized_lines(character, TextOverflow::Clip), [false; 5]);
        let ellipsized = ellipsized_lines(character, TextOverflow::Ellipsis);
        assert_eq!(ellipsized, [false, false, false, false, true]);
    }
}

// Compose's `ParagraphIntegrationLineHeightStyleTest`: 10 sp text on 20 sp lines, the test font's
// ascent 8 px and descent 2 px, its 10 px of extra height spread by the alignment (half and
// half, all below, all above, or 8 above and 2 below as the font's) and trimmed as the style
// says. Its line ascent and descent queries are internal (`AndroidParagraph`'s); the tops,
// bottoms, heights and baselines are checked, with and without soft wrapping.

/// The extra height above a line's text for `alignment`: `None` is proportional.
fn above(alignment: Option<f32>) -> f32 {
    alignment.map_or(8.0, |ratio| 10.0 * ratio)
}

/// `text` 30 px wide on 20 sp lines in `alignment` (`None`: proportional) and `trim`.
fn line_height_paragraph(
    text: &str,
    alignment: Option<f32>,
    trim: LineHeightTrim,
    soft_wrap: bool,
) -> Rc<dyn Paragraph> {
    let alignment = alignment
        .map_or(LineHeightAlignment::PROPORTIONAL, |top_ratio| LineHeightAlignment { top_ratio });
    let style = TextStyle {
        line_height: 20.sp(),
        line_height_style: Some(LineHeightStyle::new(alignment, trim)),
        ..sized(10.0)
    };
    let mut measurer = FixedTextMeasurer::with_metrics(FixedFontMetrics::SAMPLE_FONT);
    let options = TextLayoutOptions { soft_wrap, ..TextLayoutOptions::default() };
    let constraints = Constraints::new(30, 30, 0, Constraints::INFINITY);
    let handle = measurer.layout(text, &style, options, constraints, &Density::new(1.0));
    measurer.paragraph(handle)
}

const ALIGNMENTS: [Option<f32>; 4] = [Some(0.5), Some(0.0), Some(1.0), None];
const TRIMS: [LineHeightTrim; 4] = [
    LineHeightTrim::None,
    LineHeightTrim::LastLineBottom,
    LineHeightTrim::FirstLineTop,
    LineHeightTrim::Both,
];

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, singleLine_even_trim_None
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, singleLine_even_trim_LastLineBottom
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, singleLine_even_trim_FirstLineTop
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, singleLine_even_trim_Both
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, singleLine_top_trim_None
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, singleLine_top_trim_LastLineBottom
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, singleLine_top_trim_FirstLineTop
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, singleLine_top_trim_Both
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, singleLine_bottom_trim_None
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, singleLine_bottom_trim_LastLineBottom
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, singleLine_bottom_trim_FirstLineTop
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, singleLine_bottom_trim_Both
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, singleLine_proportional_trim_None
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, singleLine_proportional_trim_LastLineBottom
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, singleLine_proportional_trim_FirstLineTop
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, singleLine_proportional_trim_Both
#[test]
fn a_single_lines_height_spreads_and_trims_as_its_line_height_style_says() {
    for soft_wrap in [true, false] {
        for alignment in ALIGNMENTS {
            for trim in TRIMS {
                let paragraph = line_height_paragraph("AAA", alignment, trim, soft_wrap);
                let what = format!("{alignment:?} {trim:?} {soft_wrap}");
                assert_eq!(paragraph.line_count(), 1, "{what}");
                let top = if trim.trims_first_line_top() { 0.0 } else { above(alignment) };
                let bottom =
                    if trim.trims_last_line_bottom() { 0.0 } else { 10.0 - above(alignment) };
                assert_eq!(paragraph.get_line_top(0), 0.0, "{what}");
                assert_eq!(paragraph.get_line_bottom(0), 10.0 + top + bottom, "{what}");
                assert_eq!(paragraph.get_line_height(0), 10.0 + top + bottom, "{what}");
                assert_eq!(paragraph.height(), 10.0 + top + bottom, "{what}");
                assert_eq!(paragraph.get_line_baseline(0), 8.0 + top, "{what}");
            }
        }
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, multiLine_even_trim_None
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, multiLine_even_trim_LastLineBottom
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, multiLine_even_trim_FirstLineTop
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, multiLine_even_trim_Both
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, multiLine_top_trim_None
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, multiLine_top_trim_LastLineBottom
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, multiLine_top_trim_FirstLineTop
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, multiLine_top_trim_Both
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, multiLine_bottom_trim_None
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, multiLine_bottom_trim_LastLineBottom
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, multiLine_bottom_trim_FirstLineTop
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, multiLine_bottom_trim_Both
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, multiLine_proportional_trim_None
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, multiLine_proportional_trim_LastLineBottom
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, multiLine_proportional_trim_FirstLineTop
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, multiLine_proportional_trim_Both
#[test]
fn only_the_first_and_last_lines_are_trimmed() {
    for alignment in ALIGNMENTS {
        for trim in TRIMS {
            let paragraph = line_height_paragraph("AAAAAAAAA", alignment, trim, true);
            let what = format!("{alignment:?} {trim:?}");
            assert_eq!(paragraph.line_count(), 3, "{what}");
            let first =
                if trim.trims_first_line_top() { 10.0 + 10.0 - above(alignment) } else { 20.0 };
            let last = if trim.trims_last_line_bottom() { 10.0 + above(alignment) } else { 20.0 };
            assert_eq!(paragraph.get_line_height(0), first, "{what}");
            assert_eq!(paragraph.get_line_height(1), 20.0, "{what}");
            assert_eq!(paragraph.get_line_height(2), last, "{what}");
            let baselines: Vec<f32> =
                (0..3).map(|line| paragraph.get_line_baseline(line)).collect();
            assert_eq!(baselines[1] - baselines[0], 20.0, "{what}");
            assert_eq!(baselines[2] - baselines[1], 20.0, "{what}");
        }
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationLineHeightStyleTest.kt, lastLineEmptyTextHasSameLineHeightAsNonEmptyText
#[test]
fn an_empty_line_is_as_high_as_a_line_of_text() {
    let pairs = [
        ("", "a"),
        ("\n", "a\na"),
        ("a\n", "a\na"),
        ("\na", "a\na"),
        ("\na\na", "a\na\na"),
        ("a\na\n", "a\na\na"),
    ];
    for (empty, full) in pairs {
        let empty_paragraph = line_height_paragraph(empty, None, LineHeightTrim::None, true);
        let full_paragraph = line_height_paragraph(full, None, LineHeightTrim::None, true);
        assert_eq!(empty_paragraph.height(), full_paragraph.height(), "{empty:?}");
        for line in 0..empty_paragraph.line_count() {
            assert_eq!(empty_paragraph.get_line_top(line), full_paragraph.get_line_top(line));
            assert_eq!(empty_paragraph.get_line_bottom(line), full_paragraph.get_line_bottom(line));
            assert_eq!(
                empty_paragraph.get_line_baseline(line),
                full_paragraph.get_line_baseline(line)
            );
        }
    }
}

// Compose's `ParagraphIntrinsics` are the paragraph's own widths here: a paragraph laid out
// without bounds.
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntrinsicIntegrationTest.kt, maxIntrinsicWidth_empty_string_is_zero
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntrinsicIntegrationTest.kt, minIntrinsicWidth_empty_string_is_zero
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntrinsicIntegrationTest.kt, maxIntrinsicWidth_with_rtl_string
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntrinsicIntegrationTest.kt, maxIntrinsicWidth_with_ltr_string
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntrinsicIntegrationTest.kt, minIntrinsicWidth_with_rtl_string
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntrinsicIntegrationTest.kt, minIntrinsicWidth_with_ltr_string
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntrinsicIntegrationTest.kt, maxIntrinsicWidth_with_line_feed
#[test]
fn intrinsic_widths_of_plain_text() {
    let empty = paragraph("", sized(14.0), None);
    assert_eq!(empty.max_intrinsic_width(), 0.0);
    assert_eq!(empty.min_intrinsic_width(), 0.0);
    for text in [HEBREW, "abc"] {
        let paragraph = paragraph(text, sized(50.0), None);
        assert_eq!(paragraph.max_intrinsic_width(), 150.0, "{text}");
        assert_eq!(paragraph.min_intrinsic_width(), 150.0, "{text}");
    }
    assert_eq!(paragraph("abc\nabc", sized(50.0), None).max_intrinsic_width(), 150.0);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntrinsicIntegrationTest.kt, maxInstrinsicWidth_withStyledText
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntrinsicIntegrationTest.kt, minInstrinsicWidth_withStyledText
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntrinsicIntegrationTest.kt, minInstrinsicWidth_returns_longest_word_width
#[test]
fn intrinsic_widths_of_styled_text_and_words() {
    let styled = spanned("a bb ccc", sized(12.0), &[(span_size(24.sp()), 2, 5)]);
    assert_eq!(styled.max_intrinsic_width(), (8.0 + 3.0) * 12.0);
    let styled = spanned("a bb ccc", sized(12.0), &[(span_size(24.sp()), 1, 5)]);
    assert_eq!(styled.min_intrinsic_width(), 3.0 * 24.0);
    let text: String = (1..=50).map(|length| "a".repeat(length) + " ").collect();
    assert_eq!(paragraph(&text, sized(12.0), None).min_intrinsic_width(), 51.0 * 12.0);
}

/// A line's visible end leaves out the spaces Compose's `LayoutHelper.isLineEndSpace` names (the
/// space, the line feed, the ogham and ideographic spaces, the en to hair spaces but the figure
/// space): a tab or a figure space stays visible (D1109).
#[test]
fn a_lines_visible_end_leaves_out_compose_s_line_end_spaces() {
    let visible = |text: &str| paragraph(text, sized(10.0), None).get_line_end(0, true);
    assert_eq!(visible("abc   "), 3);
    assert_eq!(visible("abc\u{3000}\u{2003} "), 3);
    assert_eq!(visible("abc\t"), 4);
    assert_eq!(visible("abc\u{2007}"), "abc\u{2007}".len());
}
