//! `Paragraph`'s queries on SkParagraph layouts (D1105), adapted from Compose's
//! `ParagraphIntegrationTest` where its expectations don't depend on its test font: lines and
//! offsets, directions, words, and positions and boxes checked against each other and against
//! the measurer's own cursor.

use std::rc::Rc;

use compose::ui::{
    Constraints, Density, Offset, Paragraph, ResolvedTextDirection, TextOverflow, TextRange,
    TextStyle,
    platform::{TextLayoutOptions, TextMeasurer},
};
use compose_skia::SkiaTextMeasurer;

fn paragraph_with(text: &str, options: TextLayoutOptions, width: Option<i32>) -> Rc<dyn Paragraph> {
    let mut measurer = SkiaTextMeasurer::new();
    let constraints = width.map_or(Constraints::unbounded(), |width| {
        Constraints::new(width, width, 0, Constraints::INFINITY)
    });
    let handle =
        measurer.layout(text, &TextStyle::DEFAULT, options, constraints, &Density::default());
    let paragraph = measurer.paragraph(handle);
    // The queries outlive the measurer's paragraph.
    measurer.release(handle);
    paragraph
}

fn paragraph(text: &str) -> Rc<dyn Paragraph> {
    paragraph_with(text, TextLayoutOptions::default(), None)
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getLineStart_newline
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getLineEnd_newline
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getLineForOffset_withNewline
#[test]
fn skia_lines_start_and_end_at_line_feeds_in_bytes() {
    // Two-byte characters: the line metrics' UTF-16 indices are bytes here.
    let text = "\u{e9}\u{e9}\u{e9}\n\u{e8}\u{e8}\u{e8}";
    let paragraph = paragraph(text);
    assert_eq!(paragraph.line_count(), 2);
    assert_eq!(paragraph.get_line_start(0), 0);
    assert_eq!(paragraph.get_line_end(0, false), 7);
    assert_eq!(paragraph.get_line_end(0, true), 6);
    assert_eq!(paragraph.get_line_start(1), 7);
    assert_eq!(paragraph.get_line_end(1, false), text.len());
    for offset in [0, 2, 4] {
        assert_eq!(paragraph.get_line_for_offset(offset), 0);
    }
    for offset in [7, 9, 11, 13] {
        assert_eq!(paragraph.get_line_for_offset(offset), 1);
    }
    assert!(paragraph.get_line_top(1) >= paragraph.get_line_bottom(0) - 0.5);
    let middle = (paragraph.get_line_top(1) + paragraph.get_line_bottom(1)) / 2.0;
    assert_eq!(paragraph.get_line_for_vertical_position(middle), 1);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getCursorRect_ltr_singleLine
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getBoundingBox_ltr_singleLine
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getOffsetForPosition_ltr
#[test]
fn skia_positions_boxes_and_offsets_agree_left_to_right() {
    let text = "Hello world";
    let mut measurer = SkiaTextMeasurer::new();
    let options = TextLayoutOptions::default();
    let handle = measurer.layout(
        text,
        &TextStyle::DEFAULT,
        options,
        Constraints::unbounded(),
        &Density::default(),
    );
    let paragraph = measurer.paragraph(handle);
    for offset in 0..=text.len() {
        let cursor = paragraph.get_cursor_rect(offset);
        assert_eq!(cursor.left, measurer.cursor_rect(handle, offset).left, "at {offset}");
        assert_eq!(cursor.left, paragraph.get_horizontal_position(offset, false), "at {offset}");
    }
    for offset in 0..text.len() {
        let bounds = paragraph.get_bounding_box(offset);
        assert!(bounds.left <= bounds.right, "at {offset}");
        assert_eq!(bounds.left, paragraph.get_horizontal_position(offset, true), "at {offset}");
        if offset + 1 < text.len() {
            assert_eq!(bounds.right, paragraph.get_bounding_box(offset + 1).left, "at {offset}");
        }
        // A point in the character's left quarter is before it.
        let y = (bounds.top + bounds.bottom) / 2.0;
        let inside = Offset::new(bounds.left + bounds.width() / 4.0, y);
        assert_eq!(paragraph.get_offset_for_position(inside), offset, "at {offset}");
    }
    assert_eq!(paragraph.get_paragraph_direction(0), ResolvedTextDirection::Ltr);
    assert_eq!(paragraph.get_bidi_run_direction(0), ResolvedTextDirection::Ltr);
    // Laid out at its own width rounded up (Compose's `finalMaxWidth`).
    assert_eq!(paragraph.max_intrinsic_width().ceil(), paragraph.width());
    assert!(paragraph.min_intrinsic_width() < paragraph.max_intrinsic_width());
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getHorizontalPositionForOffset_primary_rtl_singleLine_textDirectionDefault
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getParagraphDirection_rtl_singleLine_textDirectionDefault
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getBidiRunDirection_rtl_singleLine_textDirectionDefault
#[test]
fn skia_right_to_left_text_runs_from_the_right() {
    let text = "\u{05D0}\u{05D1}\u{05D2}";
    let paragraph = paragraph(text);
    assert_eq!(paragraph.get_paragraph_direction(0), ResolvedTextDirection::Rtl);
    assert_eq!(paragraph.get_bidi_run_direction(0), ResolvedTextDirection::Rtl);
    let start = paragraph.get_horizontal_position(0, true);
    let middle = paragraph.get_horizontal_position(2, true);
    let end = paragraph.get_horizontal_position(text.len(), true);
    assert!(start > middle && middle > end, "{start} {middle} {end}");
    // The line's edges, as SkParagraph measures them (to its rounding).
    assert!((start - paragraph.get_line_right(0)).abs() < 0.01, "{start}");
    assert!((end - paragraph.get_line_left(0)).abs() < 0.01, "{end}");
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testGetWordBoundary
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testGetWordBoundary_spaces
#[test]
fn skia_words() {
    let paragraph = paragraph("ab cd  e");
    assert_eq!(paragraph.get_word_boundary(0), TextRange::new(0, 2));
    assert_eq!(paragraph.get_word_boundary(2), TextRange::new(0, 2));
    assert_eq!(paragraph.get_word_boundary(6), TextRange::at(6));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, getLineEllipsisOffset
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, didExceedMaxLines_ellipsis_withMaxLinesSmallerThanTextLines_returnsTrue
#[test]
fn skia_an_ellipsized_last_line_ends_at_the_texts_end() {
    let options = TextLayoutOptions {
        max_lines: 2,
        overflow: TextOverflow::Ellipsis,
        ..TextLayoutOptions::default()
    };
    let paragraph = paragraph_with("aaa\nbbb\nccc", options, Some(400));
    assert_eq!(paragraph.line_count(), 2);
    assert!(paragraph.did_exceed_max_lines());
    assert!(!paragraph.is_line_ellipsized(0));
    assert!(paragraph.is_line_ellipsized(1));
    assert_eq!(paragraph.get_line_end(0, false), 4);
    assert_eq!(paragraph.get_line_end(1, false), 11);
    assert_eq!(paragraph.get_line_for_offset(9), 1);
}
