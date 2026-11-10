//! `Paragraph`'s queries on SkParagraph layouts (D1105), adapted from Compose's
//! `ParagraphIntegrationTest` where its expectations don't depend on its test font: lines and
//! offsets, directions, words, and positions and boxes checked against each other and against
//! the measurer's own cursor.

use std::rc::Rc;

use compose::test::basic_measure_font;
use compose::ui::{
    AnnotatedRange, BaselineShift, BlendMode, Brush, Color, Constraints, Density, DrawScope,
    DrawStyle, Font, FontFamily, FontLoadingStrategy, FontOptions, FontStyle, FontWeight,
    LOCAL_FONT_FAMILY_RESOLVER, LocaleList, Offset, Paint, Paragraph, PlatformTextStyle, Rect,
    ResolvedTextDirection, Shadow, Size, SpanStyle, Stroke, StrokeCap, StrokeJoin, TextDecoration,
    TextDirection, TextGeometricTransform, TextMotion, TextOverflow, TextRange, TextStyle,
    Typeface, TypefaceLoader, UnitExt, paragraph_of_text,
    platform::{
        FixedFontMetrics, FixedTextMeasurer, PlatformFontLoader, TextLayoutOptions, TextMeasurer,
    },
};
use compose_skia::SkiaTextMeasurer;

use super::text_painter::{Picture, draw_with};

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

/// A line's visible end leaves out what Compose's `LayoutHelper.isLineEndSpace` names, not what
/// SkParagraph takes for whitespace: a tab or a figure space stays visible (D1109).
#[test]
fn skia_a_lines_visible_end_leaves_out_compose_s_line_end_spaces() {
    let visible = |text: &str| paragraph(text).get_line_end(0, true);
    assert_eq!(visible("abc   "), 3);
    assert_eq!(visible("abc\u{3000}\u{2003} "), 3);
    assert_eq!(visible("abc\t"), 4);
    assert_eq!(visible("abc\u{2007}"), "abc\u{2007}".len());
    // On a wrapped line too, and before a line feed.
    let wrapped = paragraph_with("aaaa bbbb", TextLayoutOptions::default(), Some(40));
    assert!(wrapped.line_count() > 1);
    let end = wrapped.get_line_end(0, false);
    assert_eq!(wrapped.get_line_end(0, true), end - "aaaa bbbb"[..end].ends_with(' ') as usize);
    assert_eq!(paragraph("ab\t\ncd").get_line_end(0, true), 3);
}

// ---- `AndroidParagraphTest` painting and span visuals ----

fn sample_style(font_size: f32) -> TextStyle {
    TextStyle {
        font_size: font_size.sp(),
        font_family: Some(basic_measure_font().to_font_family()),
        text_direction: Some(TextDirection::Content),
        ..TextStyle::DEFAULT
    }
}

fn laid_out_in_scope(
    scope: &DrawScope<'_>,
    text: &str,
    style: &TextStyle,
    spans: &[AnnotatedRange<SpanStyle>],
    width: i32,
) -> Rc<dyn Paragraph> {
    paragraph_of_text(
        text,
        style,
        Constraints::new(0, width, 0, Constraints::INFINITY),
        Density::new(1.0),
        scope.current_value_of(&LOCAL_FONT_FAMILY_RESOLVER),
        LocaleList::from_language_tags("en"),
        spans,
        &[],
        u32::MAX,
        TextOverflow::Clip,
    )
}

fn painted_paragraph(
    window: (i32, i32),
    text: &'static str,
    style: TextStyle,
    spans: Vec<AnnotatedRange<SpanStyle>>,
    width: i32,
    paint_fn: impl Fn(&dyn Paragraph, &mut DrawScope<'_>) + 'static,
) -> Picture {
    draw_with(window, window, 1.0, 0, move |scope, _| {
        let paragraph = laid_out_in_scope(scope, text, &style, &spans, width);
        paint_fn(&*paragraph, scope);
    })
}

fn default_paint(
    window: (i32, i32),
    text: &'static str,
    style: TextStyle,
    spans: Vec<AnnotatedRange<SpanStyle>>,
    width: i32,
) -> Picture {
    painted_paragraph(window, text, style, spans, width, |paragraph, scope| {
        paragraph.paint(scope, Color::UNSPECIFIED, None, None, None, BlendMode::SrcOver);
    })
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, draw_with_newline_and_line_break_default_values
#[test]
fn wrapped_and_explicit_line_breaks_paint_identical_lines_in_both_directions() {
    for (wrapped, broken) in [
        ("abc\ndef", "ab\nc\nde\nf"),
        (
            "\u{05D0}\u{05D1}\u{05D2}\n\u{05D3}\u{05D4}\u{05D5}",
            "\u{05D0}\u{05D1}\n\u{05D2}\n\u{05D3}\u{05D4}\n\u{05D5}",
        ),
    ] {
        let from_wrap = default_paint((100, 200), wrapped, sample_style(50.0), vec![], 100);
        let from_breaks = default_paint((100, 200), broken, sample_style(50.0), vec![], 100);
        assert!(!from_wrap.is_blank(), "{wrapped}");
        assert_eq!(from_wrap, from_breaks, "{wrapped}");
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setColorOnWholeText
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setColorOnPartOfText
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setColorTwice_lastOneOverwrite
#[test]
fn span_colors_paint_on_whole_and_partial_ranges_with_later_spans_winning() {
    let blue = Color::from_argb(0xFF00_00FF);
    let green = Color::from_argb(0xFF00_FF00);
    let style = sample_style(20.0);

    let whole = default_paint(
        (100, 20),
        "abcde",
        style.clone(),
        vec![AnnotatedRange::new(SpanStyle { color: blue, ..SpanStyle::DEFAULT }, 0, 5)],
        100,
    );
    let style_blue =
        default_paint((100, 20), "abcde", TextStyle { color: blue, ..style.clone() }, vec![], 100);
    assert_eq!(whole, style_blue);

    let part = default_paint(
        (100, 20),
        "abcde",
        style.clone(),
        vec![AnnotatedRange::new(SpanStyle { color: blue, ..SpanStyle::DEFAULT }, 0, 3)],
        100,
    );
    assert_eq!(part.pixel(2, 10), blue);
    assert_eq!(part.pixel(62, 10), Color::BLACK);

    let overwritten = default_paint(
        (100, 20),
        "abcde",
        style,
        vec![
            AnnotatedRange::new(SpanStyle { color: blue, ..SpanStyle::DEFAULT }, 0, 5),
            AnnotatedRange::new(SpanStyle { color: green, ..SpanStyle::DEFAULT }, 0, 3),
        ],
        100,
    );
    assert_eq!(overwritten.pixel(2, 10), green);
    assert_eq!(overwritten.pixel(62, 10), blue);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setBrushOnWholeText
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setSolidColorBrushOnWholeText
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setBrushOnPartOfText
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_brushSpanReceivesSize
#[test]
fn span_brushes_paint_gradients_sized_to_the_paragraph_and_solid_colors() {
    let gradient = || Brush::horizontal_gradient([Color::RED, Color::BLUE]);
    let style = sample_style(20.0);

    let whole_span = default_paint(
        (100, 20),
        "abcde",
        style.clone(),
        vec![AnnotatedRange::new(
            SpanStyle { brush: Some(gradient()), ..SpanStyle::DEFAULT },
            0,
            5,
        )],
        100,
    );
    let whole_style = default_paint(
        (100, 20),
        "abcde",
        style.clone().with_brush(Some(gradient()), f32::NAN),
        vec![],
        100,
    );
    assert_eq!(whole_span, whole_style);

    let solid_brush = default_paint(
        (100, 20),
        "abcde",
        style.clone(),
        vec![AnnotatedRange::new(
            SpanStyle { brush: Some(Brush::solid(Color::RED)), ..SpanStyle::DEFAULT },
            0,
            5,
        )],
        100,
    );
    let red_color = default_paint(
        (100, 20),
        "abcde",
        TextStyle { color: Color::RED, ..style.clone() },
        vec![],
        100,
    );
    assert_eq!(solid_brush, red_color);

    // On "abc" of "abcde" in a 200 px paragraph, the span's gradient is sized to the 200 px
    // paragraph rather than "abc"'s 60 px: its pixels on "abc" match a 200 px paragraph whose
    // whole style carries that gradient, and "de" stays black.
    let part_span = default_paint(
        (200, 20),
        "abcde",
        style.clone(),
        vec![AnnotatedRange::new(
            SpanStyle { brush: Some(gradient()), ..SpanStyle::DEFAULT },
            0,
            3,
        )],
        200,
    );
    let full_200 = default_paint(
        (200, 20),
        "abcde",
        style.clone().with_brush(Some(gradient()), f32::NAN),
        vec![],
        200,
    );
    let narrow_60 =
        default_paint((60, 20), "abc", style.with_brush(Some(gradient()), f32::NAN), vec![], 60);
    assert_eq!(part_span.pixel(42, 10), full_200.pixel(42, 10));
    assert_ne!(part_span.pixel(42, 10), narrow_60.pixel(42, 10));
    assert_eq!(part_span.pixel(62, 10), Color::BLACK);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testStyle_setTextDecorationOnWholeText_withLineThrough
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testStyle_setTextDecorationOnWholeText_withUnderline
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testStyle_setTextDecorationOnPartText_withLineThrough
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testStyle_setTextDecorationOnPartText_withUnderline
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testStyle_setTextDecoration_withLineThroughAndUnderline
#[test]
fn span_text_decorations_paint_underline_and_line_through_on_their_ranges() {
    let style = sample_style(40.0);
    let plain = default_paint((200, 50), "abcde", style.clone(), vec![], 200);
    let decorated = |decoration: TextDecoration, end: usize| {
        default_paint(
            (200, 50),
            "abcde",
            style.clone(),
            vec![AnnotatedRange::new(
                SpanStyle { text_decoration: Some(decoration), ..SpanStyle::DEFAULT },
                0,
                end,
            )],
            200,
        )
    };
    let whole_strike = decorated(TextDecoration::LINE_THROUGH, 5);
    let whole_underline = decorated(TextDecoration::UNDERLINE, 5);
    assert_ne!(whole_strike, plain);
    assert_ne!(whole_underline, plain);
    assert_ne!(whole_strike, whole_underline);

    let part_strike = decorated(TextDecoration::LINE_THROUGH, 3);
    let part_underline = decorated(TextDecoration::UNDERLINE, 3);
    let part_both = decorated(TextDecoration::LINE_THROUGH | TextDecoration::UNDERLINE, 3);
    assert_ne!(part_strike, plain);
    assert_ne!(part_strike, whole_strike);
    assert_ne!(part_underline, plain);
    assert_ne!(part_underline, whole_underline);
    assert_ne!(part_both, part_strike);
    assert_ne!(part_both, part_underline);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setBackgroundOnWholeText
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setBackgroundOnPartText
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setBackgroundTwice_lastOneOverwrite
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testSpanStyle_background_appliedAsSpan
#[test]
fn paragraph_and_span_backgrounds_paint_behind_the_glyphs_with_later_spans_winning() {
    let blue = Color::from_argb(0xFF00_00FF);
    let green = Color::from_argb(0xFF00_FF00);
    let style = sample_style(40.0);

    // At the top-right of a 40x40 triangle glyph (x=35, y=4), the glyph itself is empty, so the
    // background color shows directly.
    let style_bg = default_paint(
        (200, 40),
        "abcde",
        TextStyle { background: blue, ..style.clone() },
        vec![],
        200,
    );
    let whole_bg = default_paint(
        (200, 40),
        "abcde",
        style.clone(),
        vec![AnnotatedRange::new(SpanStyle { background: blue, ..SpanStyle::DEFAULT }, 0, 5)],
        200,
    );
    assert_eq!(style_bg.pixel(35, 4), blue);
    assert_eq!(style_bg, whole_bg);

    let part_bg = default_paint(
        (200, 40),
        "abcde",
        style.clone(),
        vec![AnnotatedRange::new(SpanStyle { background: blue, ..SpanStyle::DEFAULT }, 0, 3)],
        200,
    );
    assert_eq!(part_bg.pixel(35, 4), blue);
    assert_eq!(part_bg.pixel(155, 4), Color::WHITE);

    let overwritten = default_paint(
        (200, 40),
        "abcde",
        style,
        vec![
            AnnotatedRange::new(SpanStyle { background: blue, ..SpanStyle::DEFAULT }, 0, 5),
            AnnotatedRange::new(SpanStyle { background: green, ..SpanStyle::DEFAULT }, 0, 3),
        ],
        200,
    );
    assert_eq!(overwritten.pixel(35, 4), green);
    assert_eq!(overwritten.pixel(155, 4), blue);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setShadow
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setShadow_withZeroBlur_bitmap
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setShadowTwice_lastOnTop
#[test]
fn span_shadows_paint_at_zero_blur_as_at_the_least_blur_and_later_spans_win() {
    let style = sample_style(20.0);
    let plain = default_paint((120, 40), "abcde", style.clone(), vec![], 120);
    let with_shadow = |shadow: Shadow, end: usize| {
        default_paint(
            (120, 40),
            "abcde",
            style.clone(),
            vec![AnnotatedRange::new(
                SpanStyle { shadow: Some(shadow), ..SpanStyle::DEFAULT },
                0,
                end,
            )],
            120,
        )
    };
    let green_zero =
        Shadow { color: Color::GREEN, offset: Offset::new(1.0, 2.0), blur_radius: 0.0 };
    let green_one = Shadow { blur_radius: 1.0, ..green_zero };
    let green_three = Shadow { blur_radius: 3.0, ..green_zero };
    assert_ne!(with_shadow(green_three, 5), plain);
    assert_ne!(with_shadow(green_zero, 5), plain);
    assert_ne!(with_shadow(green_zero, 5), with_shadow(green_one, 5));
    // Kotlin's `Float.MIN_VALUE`, the least positive float.
    let green_least = Shadow { blur_radius: f32::from_bits(1), ..green_zero };
    assert_eq!(with_shadow(green_zero, 5), with_shadow(green_least, 5));

    let green_down =
        Shadow { color: Color::GREEN, offset: Offset::new(0.0, 18.0), blur_radius: 0.0 };
    let blue_down = Shadow { color: Color::BLUE, ..green_down };
    let overwritten = default_paint(
        (120, 40),
        "abcde",
        style,
        vec![
            AnnotatedRange::new(SpanStyle { shadow: Some(green_down), ..SpanStyle::DEFAULT }, 0, 5),
            AnnotatedRange::new(SpanStyle { shadow: Some(blue_down), ..SpanStyle::DEFAULT }, 0, 3),
        ],
        120,
    );
    assert_eq!(overwritten.pixel(2, 28), Color::BLUE);
    assert_eq!(overwritten.pixel(62, 28), Color::GREEN);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setDrawStyle
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setDrawStyle_FillOnTopOfStroke
#[test]
fn span_draw_styles_stroke_glyphs_and_a_nested_fill_restores_the_interior() {
    let style = sample_style(60.0);
    let stroke = DrawStyle::Stroke(Stroke {
        width: 4.0,
        miter: 4.0,
        cap: StrokeCap::Round,
        join: StrokeJoin::Bevel,
    });
    let stroked = default_paint(
        (300, 60),
        "abcde",
        style.clone(),
        vec![AnnotatedRange::new(
            SpanStyle { draw_style: Some(stroke), ..SpanStyle::DEFAULT },
            0,
            5,
        )],
        300,
    );
    // Inside glyph 0's triangle at (18, 30), a 4 px stroke leaves the interior white.
    assert_eq!(stroked.pixel(18, 30), Color::WHITE);
    assert_ne!(stroked.pixel(1, 30), Color::WHITE);

    let fill_over_stroke = default_paint(
        (300, 60),
        "abcde",
        style,
        vec![
            AnnotatedRange::new(SpanStyle { draw_style: Some(stroke), ..SpanStyle::DEFAULT }, 0, 5),
            AnnotatedRange::new(
                SpanStyle { draw_style: Some(DrawStyle::Fill), ..SpanStyle::DEFAULT },
                1,
                3,
            ),
        ],
        300,
    );
    assert_eq!(fill_over_stroke.pixel(18, 30), Color::WHITE);
    assert_eq!(fill_over_stroke.pixel(78, 30), Color::BLACK);
    assert_eq!(fill_over_stroke.pixel(198, 30), Color::WHITE);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testPaint_can_change_TextDecoration_to_Underline
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testPaint_can_change_TextDecoration_to_None
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testPaint_TextDecoration_null_should_have_no_effect
#[test]
fn painting_a_paragraph_can_add_clear_or_keep_its_text_decoration() {
    let plain_style = sample_style(40.0);
    let underlined_style =
        TextStyle { text_decoration: Some(TextDecoration::UNDERLINE), ..plain_style.clone() };
    let plain = default_paint((120, 50), "abc", plain_style.clone(), vec![], 120);
    let underlined = default_paint((120, 50), "abc", underlined_style.clone(), vec![], 120);
    assert_ne!(plain, underlined);

    let added = painted_paragraph((120, 50), "abc", plain_style, vec![], 120, |p, scope| {
        p.paint(
            scope,
            Color::UNSPECIFIED,
            None,
            Some(TextDecoration::UNDERLINE),
            None,
            BlendMode::SrcOver,
        );
    });
    assert_eq!(added, underlined);

    let cleared =
        painted_paragraph((120, 50), "abc", underlined_style.clone(), vec![], 120, |p, scope| {
            p.paint(
                scope,
                Color::UNSPECIFIED,
                None,
                Some(TextDecoration::NONE),
                None,
                BlendMode::SrcOver,
            );
        });
    assert_eq!(cleared, plain);

    let kept = painted_paragraph((120, 50), "abc", underlined_style, vec![], 120, |p, scope| {
        p.paint(scope, Color::UNSPECIFIED, None, None, None, BlendMode::SrcOver);
    });
    assert_eq!(kept, underlined);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testPaint_can_change_DrawStyle_to_Stroke
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testPaint_can_change_DrawStyle_to_Fill
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testPaint_null_drawStyle_should_be_noop
#[test]
fn painting_a_paragraph_can_stroke_fill_or_keep_its_draw_style() {
    let stroke = DrawStyle::Stroke(Stroke {
        width: 4.0,
        miter: 2.0,
        cap: StrokeCap::Square,
        join: StrokeJoin::Bevel,
    });
    let filled_style = sample_style(60.0);
    let stroked_style = TextStyle { draw_style: Some(stroke), ..filled_style.clone() };
    let filled = default_paint((180, 60), "abc", filled_style.clone(), vec![], 180);
    let stroked = default_paint((180, 60), "abc", stroked_style.clone(), vec![], 180);
    assert_ne!(filled, stroked);

    let to_stroke = painted_paragraph((180, 60), "abc", filled_style, vec![], 180, move |p, s| {
        p.paint(s, Color::UNSPECIFIED, None, None, Some(stroke), BlendMode::SrcOver);
    });
    assert_eq!(to_stroke, stroked);

    let to_fill =
        painted_paragraph((180, 60), "abc", stroked_style.clone(), vec![], 180, |p, s| {
            p.paint(s, Color::UNSPECIFIED, None, None, Some(DrawStyle::Fill), BlendMode::SrcOver);
        });
    assert_eq!(to_fill, filled);

    let kept = painted_paragraph((180, 60), "abc", stroked_style, vec![], 180, |p, s| {
        p.paint(s, Color::UNSPECIFIED, None, None, None, BlendMode::SrcOver);
    });
    assert_eq!(kept, stroked);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testPaint_cannot_change_blendMode_permanently
#[test]
fn painting_with_a_blend_mode_leaves_later_paints_in_src_over() {
    let green_style = TextStyle { color: Color::GREEN, ..sample_style(50.0) };
    let after_multiply = draw_with((100, 50), (100, 50), 1.0, 0, move |scope, _| {
        let paragraph = laid_out_in_scope(scope, "ab", &green_style, &[], 100);
        // Painted once multiplying, then over red as it was made: green on red multiplied would
        // be black.
        paragraph.paint(scope, Color::UNSPECIFIED, None, None, None, BlendMode::Multiply);
        scope.draw_rect(Rect::new(0.0, 0.0, 100.0, 50.0), Paint::fill(Color::RED));
        paragraph.paint(scope, Color::UNSPECIFIED, None, None, None, BlendMode::SrcOver);
    });
    assert_eq!(after_multiply.pixel(2, 25), Color::GREEN);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testPaint_can_change_Shadow
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testPaint_can_change_Shadow_to_None
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testPaint_can_change_Shadow_null
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testPaint_shadow_blur_with_zero_resets_to_float_min_bitmap
#[test]
fn painting_a_paragraph_can_set_clear_or_keep_its_shadow_and_zero_blur_is_the_least_blur() {
    let red_shadow = Shadow { color: Color::RED, offset: Offset::new(1.0, 2.0), blur_radius: 3.0 };
    let plain_style = sample_style(20.0);
    let shadowed_style = TextStyle { shadow: Some(red_shadow), ..plain_style.clone() };
    let plain = default_paint((100, 40), "abc", plain_style.clone(), vec![], 100);
    let shadowed = default_paint((100, 40), "abc", shadowed_style.clone(), vec![], 100);
    assert_ne!(plain, shadowed);

    let added =
        painted_paragraph((100, 40), "abc", plain_style.clone(), vec![], 100, move |p, scope| {
            p.paint(scope, Color::UNSPECIFIED, Some(red_shadow), None, None, BlendMode::SrcOver);
        });
    assert_eq!(added, shadowed);

    let cleared =
        painted_paragraph((100, 40), "abc", shadowed_style.clone(), vec![], 100, |p, scope| {
            p.paint(scope, Color::UNSPECIFIED, Some(Shadow::NONE), None, None, BlendMode::SrcOver);
        });
    assert_eq!(cleared, plain);

    let kept = painted_paragraph((100, 40), "abc", shadowed_style, vec![], 100, |p, scope| {
        p.paint(scope, Color::UNSPECIFIED, None, None, None, BlendMode::SrcOver);
    });
    assert_eq!(kept, shadowed);

    let zero_blur = Shadow { color: Color::GREEN, offset: Offset::new(1.0, 2.0), blur_radius: 0.0 };
    let shadowed_a = |shadow: Option<Shadow>| {
        default_paint((100, 40), "a", TextStyle { shadow, ..plain_style.clone() }, vec![], 100)
    };
    let zero = shadowed_a(Some(zero_blur));
    assert_ne!(zero, shadowed_a(None));
    // Kotlin's `Float.MIN_VALUE`, the least positive float.
    assert_eq!(zero, shadowed_a(Some(Shadow { blur_radius: f32::from_bits(1), ..zero_blur })));
    assert_ne!(zero, shadowed_a(Some(Shadow { blur_radius: 1.0, ..zero_blur })));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testPaint_can_change_Color
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testPaint_cannot_change_Color_to_Unspecified
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testPaint_can_change_Color_to_Transparent
#[test]
fn painting_a_paragraph_changes_color_ignores_unspecified_and_clears_with_transparent() {
    let red_style = TextStyle { color: Color::RED, ..sample_style(40.0) };
    let yellow_style = TextStyle { color: Color::YELLOW, ..sample_style(40.0) };
    let red = default_paint((120, 40), "abc", red_style.clone(), vec![], 120);
    let yellow = default_paint((120, 40), "abc", yellow_style, vec![], 120);

    let to_yellow =
        painted_paragraph((120, 40), "abc", red_style.clone(), vec![], 120, |p, scope| {
            p.paint(scope, Color::YELLOW, None, None, None, BlendMode::SrcOver);
        });
    assert_eq!(to_yellow, yellow);

    let kept_red =
        painted_paragraph((120, 40), "abc", red_style.clone(), vec![], 120, |p, scope| {
            p.paint(scope, Color::UNSPECIFIED, None, None, None, BlendMode::SrcOver);
        });
    assert_eq!(kept_red, red);

    let transparent = painted_paragraph((120, 40), "abc", red_style, vec![], 120, |p, scope| {
        p.paint(scope, Color::TRANSPARENT, None, None, None, BlendMode::SrcOver);
    });
    assert!(transparent.is_blank());
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, drawText_withUnderlineStyle_equalToUnderlinePaint
#[test]
fn an_underline_span_on_a_paragraph_paints_the_same_as_an_underline_paint() {
    let text = "\u{30EC}\u{30F3}\u{30BA}(\u{5358}\u{7126}\u{70B9})";
    let base = TextStyle { font_size: 30.0.sp(), ..TextStyle::DEFAULT };
    let spanned = default_paint(
        (600, 100),
        text,
        base.clone(),
        vec![AnnotatedRange::new(
            SpanStyle { text_decoration: Some(TextDecoration::UNDERLINE), ..SpanStyle::DEFAULT },
            0,
            text.len(),
        )],
        600,
    );
    let painted = painted_paragraph(
        (600, 100),
        text,
        TextStyle { text_decoration: Some(TextDecoration::UNDERLINE), ..base },
        vec![],
        600,
        |p, scope| {
            p.paint(
                scope,
                Color::UNSPECIFIED,
                None,
                Some(TextDecoration::UNDERLINE),
                None,
                BlendMode::SrcOver,
            );
        },
    );
    assert!(!spanned.is_blank());
    assert_eq!(spanned, painted);
}

// The sample font's glyph of a Latin letter is a triangle pointing right, from the line's top to
// its em's bottom: at (s / 4, s / 2) of a glyph of size s is its inside.

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testSpanStyle_fontSize_appliedOnTextPaint
#[test]
fn a_paragraph_s_font_size_sizes_what_it_paints() {
    let small = default_paint((200, 120), "a", sample_style(50.0), vec![], 200);
    let large = default_paint((200, 120), "a", sample_style(100.0), vec![], 200);
    assert_eq!(small.pixel(12, 25), Color::BLACK);
    assert!(small.is_empty_from(51, 0) && small.is_empty_from(0, 51));
    assert_eq!(large.pixel(25, 50), Color::BLACK);
    assert_eq!(large.pixel(60, 50), Color::BLACK);
    assert_eq!(large.pixel(5, 90), Color::BLACK);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testSpanStyle_color_appliedOnTextPaint
#[test]
fn a_paragraph_s_color_paints_its_glyphs() {
    let color = Color::from_argb(0x1234_5678);
    let text =
        default_paint((120, 40), "abc", TextStyle { color, ..sample_style(40.0) }, vec![], 120);
    let swatch = draw_with((1, 1), (1, 1), 1.0, 0, move |scope, _| {
        scope.draw_rect(Rect::new(0.0, 0.0, 1.0, 1.0), Paint::fill(color));
    });
    for glyph in 0..3 {
        assert_eq!(text.pixel(glyph * 40 + 10, 20), swatch.pixel(0, 0), "{glyph}");
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testSpanStyle_brush_appliedOnTextPaint
#[test]
fn a_paragraph_s_brush_paints_its_glyphs_across_the_paragraph() {
    let gradient = Brush::horizontal_gradient([Color::RED, Color::BLUE]);
    let style = sample_style(40.0).with_brush(Some(gradient), f32::NAN);
    let text = default_paint((120, 40), "abc", style, vec![], 120);
    let first = text.pixel(10, 20);
    let last = text.pixel(90, 20);
    assert!(first.red() > 0.7 && first.blue() < 0.3, "{first:?}");
    assert!(last.blue() > 0.6 && last.red() < 0.4, "{last:?}");
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testSpanStyle_drawStyle_stroke_appliedOnTextPaint
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testSpanStyle_drawStyle_fill_appliedOnTextPaint
#[test]
fn a_paragraph_s_draw_style_strokes_or_fills_its_glyphs() {
    let stroke = DrawStyle::Stroke(Stroke {
        width: 8.0,
        miter: 6.0,
        cap: StrokeCap::Round,
        join: StrokeJoin::Bevel,
    });
    let styled = |draw_style| {
        let style = TextStyle { draw_style, ..sample_style(100.0) };
        default_paint((300, 120), "abc", style, vec![], 300)
    };
    let plain = styled(None);
    let filled = styled(Some(DrawStyle::Fill));
    let stroked = styled(Some(stroke));
    assert_eq!(filled, plain);
    assert_eq!(filled.pixel(25, 50), Color::BLACK);
    // The inside is left out, the outline drawn on either side of the edge.
    assert_eq!(stroked.pixel(25, 50), Color::WHITE);
    assert_eq!(stroked.pixel(1, 50), Color::BLACK);
    assert!(!stroked.is_empty_from(0, 102));
    assert!(filled.is_empty_from(0, 102));
    let spanned = default_paint(
        (300, 120),
        "abc",
        sample_style(100.0),
        vec![AnnotatedRange::new(
            SpanStyle { draw_style: Some(stroke), ..SpanStyle::DEFAULT },
            0,
            3,
        )],
        300,
    );
    assert_eq!(stroked, spanned);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testSpanStyle_textDecoration_underline_appliedOnTextPaint
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testSpanStyle_textDecoration_underline_appliedAsSpan
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testSpanStyle_textDecoration_lineThrough_appliedOnTextPaint
#[test]
fn a_paragraph_s_decoration_underlines_or_strikes_its_text_as_a_span_over_it_would() {
    let decorated = |decoration: Option<TextDecoration>| {
        let style = TextStyle { text_decoration: decoration, ..sample_style(40.0) };
        default_paint((120, 50), "abc", style, vec![], 120)
    };
    let spanned = |decoration: TextDecoration| {
        let span = SpanStyle { text_decoration: Some(decoration), ..SpanStyle::DEFAULT };
        default_paint(
            (120, 50),
            "abc",
            sample_style(40.0),
            vec![AnnotatedRange::new(span, 0, 3)],
            120,
        )
    };
    let plain = decorated(None);
    let underlined = decorated(Some(TextDecoration::UNDERLINE));
    let struck = decorated(Some(TextDecoration::LINE_THROUGH));
    assert_ne!(underlined, plain);
    assert_ne!(struck, plain);
    assert_ne!(underlined, struck);
    assert_eq!(underlined, spanned(TextDecoration::UNDERLINE));
    assert_eq!(struck, spanned(TextDecoration::LINE_THROUGH));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testEmptyFontFamily
#[test]
fn a_paragraph_without_a_family_paints_in_the_default_one() {
    let without = family_paint(None, None, None);
    assert!(!without.is_blank());
    assert_eq!(without, family_paint(Some(FontFamily::Default), None, None));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testEmptyFontFamily_withBoldFontWeightSelection
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testEmptyFontFamily_withFontStyleSelection
#[test]
fn a_paragraph_without_a_family_paints_bold_or_italic_as_its_style_selects() {
    let regular = family_paint(None, None, None);
    let bold = family_paint(None, Some(FontWeight::BOLD), None);
    let italic = family_paint(None, None, Some(FontStyle::Italic));
    assert_ne!(bold, regular);
    assert_ne!(italic, regular);
    assert_ne!(bold, italic);
    // The weight and the style are each their own: the bold isn't slanted, the italic isn't bold.
    assert_eq!(bold, family_paint(None, Some(FontWeight::BOLD), Some(FontStyle::Normal)));
    assert_eq!(italic, family_paint(None, Some(FontWeight::NORMAL), Some(FontStyle::Italic)));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testFontFamily_withGenericFamilyName
#[test]
fn a_generic_family_paints_upright_at_the_normal_weight() {
    let sans = family_paint(Some(FontFamily::SansSerif), None, None);
    assert!(!sans.is_blank());
    assert_eq!(
        sans,
        family_paint(
            Some(FontFamily::SansSerif),
            Some(FontWeight::NORMAL),
            Some(FontStyle::Normal)
        )
    );
    assert_ne!(sans, family_paint(Some(FontFamily::SansSerif), Some(FontWeight::BOLD), None));
    assert_ne!(sans, family_paint(Some(FontFamily::SansSerif), None, Some(FontStyle::Italic)));
}

/// "Abc" in the system's fonts, in `family`, `weight` and `style`.
fn family_paint(
    family: Option<FontFamily>,
    weight: Option<FontWeight>,
    style: Option<FontStyle>,
) -> Picture {
    let style = TextStyle {
        font_size: 30.0.sp(),
        font_family: family,
        font_weight: weight,
        font_style: style,
        ..TextStyle::DEFAULT
    };
    default_paint((200, 50), "Abc", style, vec![], 200)
}

/// A font an application loads itself, blocking: the sample font.
struct BlockingSampleFont;

impl TypefaceLoader for BlockingSampleFont {
    fn load_blocking(
        &self,
        platform: &dyn PlatformFontLoader,
        _: &Font,
    ) -> Result<Option<Typeface>, String> {
        platform.load_blocking(&basic_measure_font())
    }

    fn await_load(
        self: Rc<Self>,
        _: Rc<dyn PlatformFontLoader>,
        _: Font,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<Typeface>, String>>>>
    {
        Box::pin(async { Err("a blocking font".to_string()) })
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testFontFamily_withCustomFont
#[test]
fn a_paragraph_in_a_custom_font_paints_the_typeface_its_loader_gives() {
    let font = Font::from_loader(
        Rc::new(BlockingSampleFont),
        FontOptions {
            loading_strategy: Some(FontLoadingStrategy::Blocking),
            ..FontOptions::default()
        },
    );
    let custom = TextStyle { font_family: Some(font.to_font_family()), ..sample_style(40.0) };
    let loaded = default_paint((120, 50), "abc", custom, vec![], 120);
    assert_eq!(loaded, default_paint((120, 50), "abc", sample_style(40.0), vec![], 120));
    let system = TextStyle { font_family: None, ..sample_style(40.0) };
    assert_ne!(loaded, default_paint((120, 50), "abc", system, vec![], 120));
}

// ---- `AndroidParagraphTest` text-style-model & `FontPaddingTest` ----

fn painted_span(text: &'static str, span: SpanStyle, start: usize, end: usize) -> Picture {
    default_paint(
        (200, 80),
        text,
        sample_style(40.0),
        vec![AnnotatedRange::new(span, start, end)],
        200,
    )
}

fn first_painted_y_bounds(
    picture: &Picture,
    x_range: std::ops::Range<i32>,
    height: i32,
) -> (i32, i32) {
    let (mut top, mut bottom) = (i32::MAX, i32::MIN);
    for y in 0..height {
        for x in x_range.clone() {
            if picture.pixel(x, y) != Color::WHITE {
                top = top.min(y);
                bottom = bottom.max(y);
            }
        }
    }
    (top, bottom)
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setBaselineShiftOnWholeText
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setBaselineShiftOnPartText
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testSpanStyle_baselineShift_appliedAsSpan
#[test]
fn baseline_shift_moves_superscript_up_and_subscript_down_for_whole_text_or_spans() {
    let base = sample_style(40.0);
    let sup = SpanStyle { baseline_shift: Some(BaselineShift::SUPERSCRIPT), ..SpanStyle::DEFAULT };
    let sub = SpanStyle { baseline_shift: Some(BaselineShift::SUBSCRIPT), ..SpanStyle::DEFAULT };

    // Whole text in a span matches the paragraph's own `TextStyle`.
    let whole_sub = painted_span("abcde", sub.clone(), 0, 5);
    let style_sub = default_paint(
        (200, 80),
        "abcde",
        TextStyle { baseline_shift: Some(BaselineShift::SUBSCRIPT), ..base.clone() },
        vec![],
        200,
    );
    assert_eq!(whole_sub, style_sub);

    // Partial span: "a" shifted up/down relative to plain "b".
    let part_sup = painted_span("ab", sup, 0, 1);
    let (sup_top, sup_bottom) = first_painted_y_bounds(&part_sup, 0..40, 80);
    let (plain_after_sup_top, plain_after_sup_bottom) =
        first_painted_y_bounds(&part_sup, 40..80, 80);
    assert!(sup_top < plain_after_sup_top && sup_bottom < plain_after_sup_bottom);

    let part_sub = painted_span("ab", sub, 0, 1);
    let (sub_top, sub_bottom) = first_painted_y_bounds(&part_sub, 0..40, 80);
    let (plain_after_sub_top, plain_after_sub_bottom) =
        first_painted_y_bounds(&part_sub, 40..80, 80);
    assert!(sub_top > plain_after_sub_top && sub_bottom > plain_after_sub_bottom);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setBaselineShiftTwice_LastOneOnTop
#[test]
fn overlapping_baseline_shifts_stack_from_outer_to_inner_and_opposite_shifts_cancel() {
    let sub = SpanStyle { baseline_shift: Some(BaselineShift::SUBSCRIPT), ..SpanStyle::DEFAULT };
    let sup = SpanStyle { baseline_shift: Some(BaselineShift::SUPERSCRIPT), ..SpanStyle::DEFAULT };
    // Subscript across "abcde", Superscript on "abc" cancels to unshifted while "de" stays low.
    let stacked = default_paint(
        (200, 80),
        "abcde",
        sample_style(40.0),
        vec![AnnotatedRange::new(sub, 0, 5), AnnotatedRange::new(sup, 0, 3)],
        200,
    );
    let (cancelled_top, cancelled_bottom) = first_painted_y_bounds(&stacked, 0..120, 80);
    let (remaining_top, remaining_bottom) = first_painted_y_bounds(&stacked, 120..200, 80);
    assert!(cancelled_top < remaining_top && cancelled_bottom < remaining_bottom);

    // Two positive shifts stack higher than one.
    let quarter = SpanStyle { baseline_shift: Some(BaselineShift(0.25)), ..SpanStyle::DEFAULT };
    let doubled = default_paint(
        (120, 80),
        "ab",
        sample_style(40.0),
        vec![AnnotatedRange::new(quarter.clone(), 0, 2), AnnotatedRange::new(quarter, 0, 1)],
        120,
    );
    let (first_top, _) = first_painted_y_bounds(&doubled, 0..40, 80);
    let (second_top, _) = first_painted_y_bounds(&doubled, 40..80, 80);
    assert!(first_top < second_top);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setBaselineShiftNone_doesNotAddSpan
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setBaselineShiftUnspecified_doesNotAddSpan
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setBaselineShiftPositiveInfinity_doesNotAddSpan
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setBaselineShiftNegativeInfinity_doesNotAddSpan
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setBaselineShiftCustomZeroMultiplier_doesNotAddSpan
#[test]
fn non_applicable_baseline_shifts_leave_text_unshifted() {
    let plain = default_paint((200, 80), "abcde", sample_style(40.0), vec![], 200);
    for shift in [
        BaselineShift::NONE,
        BaselineShift::UNSPECIFIED,
        BaselineShift(f32::INFINITY),
        BaselineShift(f32::NEG_INFINITY),
        BaselineShift(0.0),
    ] {
        let span = SpanStyle { baseline_shift: Some(shift), ..SpanStyle::DEFAULT };
        assert_eq!(painted_span("abcde", span, 0, 5), plain, "{shift:?}");
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setDefaultTextGeometricTransform
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setTextGeometricTransformWithScaleX
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setTextGeometricTransformWithSkewX
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testSpanStyle_scaleX_appliedOnTextPaint
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testSpanStyle_skewX_appliedOnTextPaint
#[test]
fn geometric_transform_scales_and_skews_glyphs_across_styles_and_spans() {
    let base = sample_style(40.0);
    let plain = default_paint((240, 50), "abcde", base.clone(), vec![], 240);
    let default_transform = default_paint(
        (240, 50),
        "abcde",
        base.clone(),
        vec![AnnotatedRange::new(
            SpanStyle {
                text_geometric_transform: Some(TextGeometricTransform::NONE),
                ..SpanStyle::DEFAULT
            },
            0,
            5,
        )],
        240,
    );
    assert_eq!(default_transform, plain);

    // scaleX = 0.5 halves glyph widths while preserving height; TextStyle and SpanStyle agree.
    let half = TextGeometricTransform { scale_x: 0.5, skew_x: 0.0 };
    let span_half = default_paint(
        (240, 50),
        "abcde",
        base.clone(),
        vec![AnnotatedRange::new(
            SpanStyle { text_geometric_transform: Some(half), ..SpanStyle::DEFAULT },
            0,
            5,
        )],
        240,
    );
    let style_half = default_paint(
        (240, 50),
        "abcde",
        TextStyle { text_geometric_transform: Some(half), ..base.clone() },
        vec![],
        240,
    );
    assert_eq!(span_half, style_half);
    assert_ne!(span_half, plain);
    // Five 40 px glyphs scaled by 0.5 end at x = 100, leaving x >= 101 blank.
    assert!(!span_half.is_empty_from(0, 0) && span_half.is_empty_from(101, 0));

    // skewX slants glyphs horizontally; TextStyle and SpanStyle agree.
    let skew = TextGeometricTransform { scale_x: 1.0, skew_x: 0.5 };
    let plain_spaced = default_paint((200, 50), " aa", base.clone(), vec![], 200);
    let span_skew = default_paint(
        (200, 50),
        " aa",
        base.clone(),
        vec![AnnotatedRange::new(
            SpanStyle { text_geometric_transform: Some(skew), ..SpanStyle::DEFAULT },
            0,
            3,
        )],
        200,
    );
    let style_skew = default_paint(
        (200, 50),
        " aa",
        TextStyle { text_geometric_transform: Some(skew), ..base.clone() },
        vec![],
        200,
    );
    assert_eq!(span_skew, style_skew);
    assert_ne!(span_skew, plain_spaced);
    // Positive skewX leans the top of glyph 1 ("a", starting at x=40) leftward into x < 40.
    assert_ne!(span_skew.pixel(30, 5), Color::WHITE);
    assert_eq!(plain_spaced.pixel(30, 5), Color::WHITE);

    // Transformed glyphs keep gradients, shadows, and underlines.
    let gradient = Brush::horizontal_gradient([Color::RED, Color::BLUE]);
    let brushed = default_paint(
        (160, 50),
        "aa",
        TextStyle {
            text_geometric_transform: Some(TextGeometricTransform { scale_x: 2.0, skew_x: 0.0 }),
            text_decoration: Some(TextDecoration::UNDERLINE),
            shadow: Some(Shadow {
                color: Color::GREEN,
                offset: Offset::new(0.0, 10.0),
                blur_radius: 0.0,
            }),
            ..base
        }
        .with_brush(Some(gradient), f32::NAN),
        vec![],
        160,
    );
    assert!(brushed.contains(Color::GREEN));
    assert!(!brushed.is_empty_from(120, 0));
}

/// A minimal TTF with a `GSUB` table (`liga`: `'f'+'i'` -> full square; `locl` under `latn`:
/// `SRB ` maps `'a'` -> top bar, `JAN ` maps `'a'` -> bottom bar), plus a tall variant with twice
/// the vertical metrics (`ascent`/`descent`).
fn custom_opentype_font(tall: bool) -> Font {
    static REGULAR: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    static TALL: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    let bytes = if tall { &TALL } else { &REGULAR }.get_or_init(|| {
        #[derive(Default)]
        struct T(Vec<u8>);
        impl T {
            fn u16(mut self, v: u16) -> Self {
                self.0.extend_from_slice(&v.to_be_bytes());
                self
            }
            fn i16(self, v: i16) -> Self {
                self.u16(v as u16)
            }
            fn u32(mut self, v: u32) -> Self {
                self.0.extend_from_slice(&v.to_be_bytes());
                self
            }
            fn zeros(mut self, n: usize) -> Self {
                self.0.resize(self.0.len() + n, 0);
                self
            }
            fn tag(mut self, tag: &[u8; 4]) -> Self {
                self.0.extend_from_slice(tag);
                self
            }
        }

        let (asc, desc, win_asc, win_desc): (i16, i16, u16, u16) =
            if tall { (1600, -400, 2000, 400) } else { (800, -200, 1000, 200) };
        let glyphs: [(u16, &[(i16, i16)]); 8] = [
            (500, &[]),
            (1000, &[(0, desc), (0, asc), (1000, (asc + desc) / 2)]),
            (1000, &[(0, desc), (0, asc), (1000, (asc + desc) / 2)]),
            (1000, &[(0, 0), (0, 80), (1000, 80), (1000, 0)]),
            (1000, &[(0, desc), (0, asc), (1000, asc), (1000, desc)]),
            (1000, &[(0, asc / 2), (0, asc), (1000, asc), (1000, asc / 2)]),
            (1000, &[(0, desc), (0, 0), (1000, 0), (1000, desc)]),
            (1000, &[]),
        ];
        let chars: [(u32, u16); 5] = [(0x0A, 7), (0x20, 7), (0x61, 1), (0x66, 2), (0x69, 3)];

        let head = T::default()
            .u32(0x0001_0000)
            .u32(0x0001_0000)
            .u32(0)
            .u32(0x5F0F_3CF5)
            .u16(0b11)
            .u16(1000)
            .zeros(16)
            .i16(0)
            .i16(desc)
            .i16(1000)
            .i16(asc)
            .u16(0)
            .u16(8)
            .i16(2)
            .i16(1)
            .i16(0)
            .0;
        let hhea = T::default()
            .u32(0x0001_0000)
            .i16(asc)
            .i16(desc)
            .i16(0)
            .u16(1000)
            .i16(0)
            .i16(0)
            .i16(1000)
            .i16(1)
            .zeros(12)
            .i16(0)
            .u16(glyphs.len() as u16)
            .0;
        let maxp = T::default()
            .u32(0x0001_0000)
            .u16(glyphs.len() as u16)
            .u16(4)
            .u16(1)
            .u16(0)
            .u16(0)
            .u16(1)
            .zeros(16)
            .0;
        let os2 = T::default()
            .u16(4)
            .i16(1000)
            .u16(400)
            .u16(5)
            .u16(0)
            .zeros(22 + 10 + 16 + 4)
            .u16(0x40)
            .u16(0x0A)
            .u16(0x69)
            .i16(asc)
            .i16(desc)
            .i16(200)
            .u16(win_asc)
            .u16(win_desc)
            .zeros(8 + 4 + 6)
            .0;
        let mut hmtx = T::default();
        for (adv, _) in &glyphs {
            hmtx = hmtx.u16(*adv).i16(0);
        }
        let mut cmap = T::default()
            .u16(0)
            .u16(2)
            .u16(0)
            .u16(4)
            .u32(20)
            .u16(3)
            .u16(10)
            .u32(20)
            .u16(12)
            .u16(0)
            .u32(16 + chars.len() as u32 * 12)
            .u32(0)
            .u32(chars.len() as u32);
        for (ch, g) in &chars {
            cmap = cmap.u32(*ch).u32(*ch).u32(u32::from(*g));
        }
        let mut glyf = T::default();
        let mut loca = T::default().u32(0);
        let mut end = 0u32;
        for (_, pts) in &glyphs {
            if !pts.is_empty() {
                let start = glyf.0.len();
                let xs = pts.iter().map(|p| p.0);
                let ys = pts.iter().map(|p| p.1);
                glyf = glyf
                    .i16(1)
                    .i16(xs.clone().min().unwrap_or(0))
                    .i16(ys.clone().min().unwrap_or(0))
                    .i16(xs.max().unwrap_or(0))
                    .i16(ys.max().unwrap_or(0))
                    .u16(pts.len() as u16 - 1)
                    .u16(0);
                glyf.0.extend(std::iter::repeat_n(1u8, pts.len()));
                let mut last = 0;
                for p in *pts {
                    glyf = glyf.i16(p.0 - last);
                    last = p.0;
                }
                last = 0;
                for p in *pts {
                    glyf = glyf.i16(p.1 - last);
                    last = p.1;
                }
                let padded = (glyf.0.len() - start).next_multiple_of(4);
                glyf.0.resize(start + padded, 0);
                end = glyf.0.len() as u32;
            }
            loca = loca.u32(end);
        }
        let label = if tall { "Compose Tall" } else { "Compose Feature" };
        let enc: Vec<u8> = label.encode_utf16().flat_map(|u| u.to_be_bytes()).collect();
        let mut name = T::default()
            .u16(0)
            .u16(1)
            .u16(18)
            .u16(3)
            .u16(1)
            .u16(0x409)
            .u16(1)
            .u16(enc.len() as u16)
            .u16(0);
        name.0.extend_from_slice(&enc);
        let post = T::default().u32(0x0003_0000).u32(0).i16(-100).i16(50).zeros(20).0;

        // GSUB v1.0:
        // Header(10): scriptList=10, featureList=62, lookupList=100.
        let gsub = T::default()
            .u32(0x0001_0000)
            .u16(10)
            .u16(62)
            .u16(100)
            // ScriptList (52 bytes, offset 10..62):
            .u16(1)
            .tag(b"latn")
            .u16(8)
            // Script (at ScriptList+8): default=16, count=2, JAN=24, SRB=34
            .u16(16)
            .u16(2)
            .tag(b"JAN ")
            .u16(24)
            .tag(b"SRB ")
            .u16(34)
            // DefaultLangSys (8 bytes): feature [0]
            .u16(0)
            .u16(0xFFFF)
            .u16(1)
            .u16(0)
            // JAN LangSys (10 bytes): features [0, 2]
            .u16(0)
            .u16(0xFFFF)
            .u16(2)
            .u16(0)
            .u16(2)
            // SRB LangSys (10 bytes): features [0, 1]
            .u16(0)
            .u16(0xFFFF)
            .u16(2)
            .u16(0)
            .u16(1)
            // FeatureList (38 bytes, offset 62..100):
            .u16(3)
            .tag(b"liga")
            .u16(20)
            .tag(b"locl")
            .u16(26)
            .tag(b"locl")
            .u16(32)
            .u16(0)
            .u16(1)
            .u16(0)
            .u16(0)
            .u16(1)
            .u16(1)
            .u16(0)
            .u16(1)
            .u16(2)
            // LookupList (at 100): 3 lookups at 8, 40, 62
            .u16(3)
            .u16(8)
            .u16(40)
            .u16(62)
            // Lookup 0 (32 bytes): LigatureSubst (type 4) 'f'(2) + 'i'(3) -> glyph 4
            .u16(4)
            .u16(0)
            .u16(1)
            .u16(8)
            .u16(1)
            .u16(8)
            .u16(1)
            .u16(14)
            .u16(1)
            .u16(1)
            .u16(2)
            .u16(1)
            .u16(4)
            .u16(4)
            .u16(2)
            .u16(3)
            // Lookup 1 (22 bytes): SingleSubst2 (type 1) 'a'(1) -> glyph 5 (SRB)
            .u16(1)
            .u16(0)
            .u16(1)
            .u16(8)
            .u16(2)
            .u16(8)
            .u16(1)
            .u16(5)
            .u16(1)
            .u16(1)
            .u16(1)
            // Lookup 2 (22 bytes): SingleSubst2 (type 1) 'a'(1) -> glyph 6 (JAN)
            .u16(1)
            .u16(0)
            .u16(1)
            .u16(8)
            .u16(2)
            .u16(8)
            .u16(1)
            .u16(6)
            .u16(1)
            .u16(1)
            .u16(1)
            .0;

        let tables: [(&[u8; 4], Vec<u8>); 11] = [
            (b"GSUB", gsub),
            (b"OS/2", os2),
            (b"cmap", cmap.0),
            (b"glyf", glyf.0),
            (b"head", head),
            (b"hhea", hhea),
            (b"hmtx", hmtx.0),
            (b"loca", loca.0),
            (b"maxp", maxp),
            (b"name", name.0),
            (b"post", post),
        ];
        let checksum = |table: &[u8]| {
            table.chunks(4).fold(0u32, |sum, word| {
                let mut pad = [0; 4];
                pad[..word.len()].copy_from_slice(word);
                sum.wrapping_add(u32::from_be_bytes(pad))
            })
        };
        let count = tables.len() as u16;
        let pow = 1u16 << (15 - count.leading_zeros());
        let mut dir = T::default()
            .u32(0x0001_0000)
            .u16(count)
            .u16(pow * 16)
            .u16(pow.trailing_zeros() as u16)
            .u16((count - pow) * 16);
        let mut offset = 12 + tables.len() * 16;
        let mut head_offset = 0;
        let mut body = Vec::new();
        for (tag, table) in &tables {
            dir = dir.tag(tag).u32(checksum(table)).u32(offset as u32).u32(table.len() as u32);
            if *tag == b"head" {
                head_offset = offset;
            }
            body.extend_from_slice(table);
            let padded = table.len().next_multiple_of(4);
            body.resize(body.len() + padded - table.len(), 0);
            offset += padded;
        }
        let mut file = dir.0;
        file.extend_from_slice(&body);
        let adj = 0xB1B0_AFBAu32.wrapping_sub(checksum(&file));
        file[head_offset + 8..head_offset + 12].copy_from_slice(&adj.to_be_bytes());
        file
    });
    Font::from_bytes(bytes.as_slice(), FontOptions::default())
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_fontFeatureSetting_setSpanOnText
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testSpanStyle_fontFeatureSettings_appliedOnTextPaint
#[test]
fn font_feature_settings_shape_whole_paragraphs_and_spans() {
    let font = custom_opentype_font(false);
    let base = TextStyle {
        font_size: 40.0.sp(),
        font_family: Some(font.to_font_family()),
        ..TextStyle::DEFAULT
    };
    // With default "liga", "fi" forms one 40 px ligature square; with `"liga" 0`, two glyphs (80 px).
    let ligated = default_paint((160, 50), "fi", base.clone(), vec![], 160);
    let unligated_style = default_paint(
        (160, 50),
        "fi",
        TextStyle { font_feature_settings: Some("\"liga\" 0".into()), ..base.clone() },
        vec![],
        160,
    );
    let unligated_span = default_paint(
        (160, 50),
        "fi",
        base.clone(),
        vec![AnnotatedRange::new(
            SpanStyle { font_feature_settings: Some("\"liga\" 0".into()), ..SpanStyle::DEFAULT },
            0,
            2,
        )],
        160,
    );
    assert_eq!(unligated_style, unligated_span);
    assert_ne!(ligated, unligated_span);
    assert!(ligated.is_empty_from(41, 0));
    assert!(!unligated_span.is_empty_from(41, 0));

    // Partial span disables ligatures on the first "fi" while keeping the second ligated (total 120 px).
    let mixed = default_paint(
        (160, 50),
        "fifi",
        base,
        vec![AnnotatedRange::new(
            SpanStyle { font_feature_settings: Some("\"liga\" 0".into()), ..SpanStyle::DEFAULT },
            0,
            2,
        )],
        160,
    );
    assert!(!mixed.is_empty_from(81, 0) && mixed.is_empty_from(121, 0));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setLocaleOnWholeText
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setLocaleOnPartText
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testAnnotatedString_setLocaleTwice_lastOneOverwrite
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, testSpanStyle_locale_appliedOnTextPaint
#[test]
fn locale_lists_shape_whole_paragraphs_spans_and_overwriting_inner_spans() {
    let font = custom_opentype_font(false);
    let base = TextStyle {
        font_size: 40.0.sp(),
        font_family: Some(font.to_font_family()),
        ..TextStyle::DEFAULT
    };
    let sr = LocaleList::from_language_tags("sr");
    let ja = LocaleList::from_language_tags("ja");

    let default_a = default_paint((120, 50), "aa", base.clone(), vec![], 120);
    let sr_style = default_paint(
        (120, 50),
        "aa",
        TextStyle { locale_list: Some(sr.clone()), ..base.clone() },
        vec![],
        120,
    );
    let sr_whole_span = default_paint(
        (120, 50),
        "aa",
        base.clone(),
        vec![AnnotatedRange::new(
            SpanStyle { locale_list: Some(sr.clone()), ..SpanStyle::DEFAULT },
            0,
            2,
        )],
        120,
    );
    assert_eq!(sr_style, sr_whole_span);
    assert_ne!(sr_style, default_a);

    // Partial span: first "a" shaped with "sr" (top bar at y=4), second "a" default (empty at x=75, y=4).
    let sr_part = default_paint(
        (120, 50),
        "aa",
        base.clone(),
        vec![AnnotatedRange::new(
            SpanStyle { locale_list: Some(sr.clone()), ..SpanStyle::DEFAULT },
            0,
            1,
        )],
        120,
    );
    assert_eq!(sr_part.pixel(35, 4), Color::BLACK);
    assert_eq!(sr_part.pixel(75, 4), Color::WHITE);

    // Later inner span overwrites: "ja" on 0..1 (bottom bar: empty at y=4, black at y=36), "sr" on 1..2.
    let overwritten = default_paint(
        (120, 50),
        "aa",
        base,
        vec![
            AnnotatedRange::new(SpanStyle { locale_list: Some(sr), ..SpanStyle::DEFAULT }, 0, 2),
            AnnotatedRange::new(SpanStyle { locale_list: Some(ja), ..SpanStyle::DEFAULT }, 0, 1),
        ],
        120,
    );
    assert_eq!(overwritten.pixel(35, 4), Color::WHITE);
    assert_eq!(overwritten.pixel(35, 36), Color::BLACK);
    assert_eq!(overwritten.pixel(75, 4), Color::BLACK);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, textMotionStatic_setsCorrectFlagsOnTextPaint
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, textMotionAnimated_setsCorrectFlagsOnTextPaint
#[test]
fn text_motion_switches_between_static_hinted_and_animated_subpixel_rendering() {
    let static_style = TextStyle {
        font_size: 15.0.sp(),
        font_family: Some(FontFamily::SansSerif),
        text_motion: Some(TextMotion::Static),
        ..TextStyle::DEFAULT
    };
    let animated_style =
        TextStyle { text_motion: Some(TextMotion::Animated), ..static_style.clone() };
    let still = default_paint((200, 40), "Hello world 12345", static_style.clone(), vec![], 200);
    let animated =
        default_paint((200, 40), "Hello world 12345", animated_style.clone(), vec![], 200);
    assert_ne!(still, animated);

    // Switching TextMotion on a single measurer updates glyph positions (not stale-cached).
    let mut measurer = SkiaTextMeasurer::new();
    let options = TextLayoutOptions::default();
    let constraints = Constraints::unbounded();
    let density = Density::new(1.0);
    let h_still =
        measurer.layout("Hello world 12345", &static_style, options, constraints, &density);
    let h_anim =
        measurer.layout("Hello world 12345", &animated_style, options, constraints, &density);
    let w_still = measurer.paragraph(h_still).max_intrinsic_width();
    let w_anim = measurer.paragraph(h_anim).max_intrinsic_width();
    assert_eq!(w_still.fract(), 0.0, "Static snaps advances: {w_still}");
    assert_ne!(w_still, w_anim);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, withIncludeFontPadding
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/FontPaddingTest.kt, latinIncludeFontPaddingDoesNotAffectLineMetricsSingleLine
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/FontPaddingTest.kt, latinIncludeFontPaddingDoesNotAffectLineMetricsMultiLine
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/FontPaddingTest.kt, latinIncludeFontPaddingDoesNotAffectGetLineForVertical
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/FontPaddingTest.kt, tallIincludeFontPaddingDoesNotAffectLineMetricsSingleLine
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/FontPaddingTest.kt, tallIncludeFontPaddingDoesNotAffectLineMetricsMultiLine
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/FontPaddingTest.kt, tallIncludeFontPaddingDoesNotAffectGetLineForVertical
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/FontPaddingTest.kt, tallTypefaceTextIsTwiceTheHeightOfLatinTypefaceTextSingleLine
// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/android/FontPaddingTest.kt, tallTypefaceTextIsTwiceTheHeightOfLatinTypefaceTextMultiLine
#[test]
fn font_padding_and_tall_font_metrics_on_skia_and_fixed_measurers() {
    let latin = custom_opentype_font(false);
    let tall = custom_opentype_font(true);
    let layout = |measurer: &mut dyn TextMeasurer, font: &Font, text: &str, include_pad: bool| {
        let style = TextStyle {
            font_size: 100.0.sp(),
            font_family: Some(font.to_font_family()),
            platform_style: Some(PlatformTextStyle::with_include_font_padding(include_pad)),
            ..TextStyle::DEFAULT
        };
        let handle = measurer.layout(
            text,
            &style,
            TextLayoutOptions::default(),
            Constraints::new(0, 150, 0, Constraints::INFINITY),
            &Density::new(1.0),
        );
        let paragraph = measurer.paragraph(handle);
        measurer.release(handle);
        paragraph
    };

    // Desktop/Skia has no Android font padding (D2306), so includeFontPadding leaves metrics alone.
    for (font, single, multi) in [(&latin, "a", "a\na\na"), (&tall, "a", "a\na\na")] {
        for text in [single, multi] {
            let mut skia = SkiaTextMeasurer::new();
            let with_pad = layout(&mut skia, font, text, true);
            let without_pad = layout(&mut skia, font, text, false);
            assert_eq!(with_pad.height(), without_pad.height());
            for line in 0..with_pad.line_count() {
                assert_eq!(with_pad.get_line_top(line), without_pad.get_line_top(line));
                assert_eq!(with_pad.get_line_baseline(line), without_pad.get_line_baseline(line));
                assert_eq!(with_pad.get_line_bottom(line), without_pad.get_line_bottom(line));
            }
            let mid = (with_pad.height() / 2.0) as i32 as f32;
            assert_eq!(
                with_pad.get_line_for_vertical_position(mid),
                without_pad.get_line_for_vertical_position(mid)
            );
        }
    }

    // Tall font (ascent 1600, descent -400) has twice the height of Latin (800, -200).
    for text in ["a", "a\na\na"] {
        let mut skia = SkiaTextMeasurer::new();
        let latin_p = layout(&mut skia, &latin, text, false);
        let tall_p = layout(&mut skia, &tall, text, false);
        assert_eq!(tall_p.height(), 2.0 * latin_p.height());

        // FixedTextMeasurer uses fixed font metrics rather than the custom font's, while keeping
        // includeFontPadding a no-op.
        let mut fixed = FixedTextMeasurer::with_metrics(FixedFontMetrics::SAMPLE_FONT);
        assert_eq!(
            layout(&mut fixed, &latin, text, true).height(),
            layout(&mut fixed, &latin, text, false).height()
        );
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, shaderBrushSpan_createsShaderOnlyOnce
#[test]
fn shader_brush_span_creates_shader_only_once() {
    use compose::ui::{CustomShaderBrush, Shader, ShaderBrush, TileMode};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Clone)]
    struct CountingSpanBrush(Arc<AtomicUsize>);

    impl CustomShaderBrush for CountingSpanBrush {
        fn create_shader(&self, size: Size) -> Shader {
            self.0.fetch_add(1, Ordering::Relaxed);
            Shader::linear_gradient(
                Offset::ZERO,
                Offset::new(size.width.max(1.0), size.height.max(1.0)),
                [Color::RED, Color::BLUE],
                TileMode::Clamp,
            )
        }
    }

    let calls = Arc::new(AtomicUsize::new(0));
    let calls_for_draw = calls.clone();
    let brush = Brush::from(ShaderBrush::custom(CountingSpanBrush(calls.clone())));
    let span = AnnotatedRange::new(SpanStyle::DEFAULT.with_brush(Some(brush), 1.0), 0, 3);
    let _ = draw_with((120, 50), (120, 50), 1.0, 0, move |scope, _| {
        let paragraph =
            laid_out_in_scope(scope, "abc", &sample_style(40.0), std::slice::from_ref(&span), 120);
        for _ in 0..5 {
            paragraph.paint(scope, Color::UNSPECIFIED, None, None, None, BlendMode::SrcOver);
        }
        assert_eq!(calls_for_draw.load(Ordering::Relaxed), 1);
    });
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}
