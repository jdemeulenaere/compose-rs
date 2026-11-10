//! `Paragraph`'s queries on SkParagraph layouts (D1105), adapted from Compose's
//! `ParagraphIntegrationTest` where its expectations don't depend on its test font: lines and
//! offsets, directions, words, and positions and boxes checked against each other and against
//! the measurer's own cursor.

use std::rc::Rc;

use compose::test::basic_measure_font;
use compose::ui::{
    AnnotatedRange, BlendMode, Brush, Color, Constraints, Density, DrawScope, DrawStyle, Font,
    FontFamily, FontLoadingStrategy, FontOptions, FontStyle, FontWeight,
    LOCAL_FONT_FAMILY_RESOLVER, LocaleList, Offset, Paint, Paragraph, Rect, ResolvedTextDirection,
    Shadow, SpanStyle, Stroke, StrokeCap, StrokeJoin, TextDecoration, TextDirection, TextOverflow,
    TextRange, TextStyle, Typeface, TypefaceLoader, UnitExt, paragraph_of_text,
    platform::{PlatformFontLoader, TextLayoutOptions, TextMeasurer},
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
