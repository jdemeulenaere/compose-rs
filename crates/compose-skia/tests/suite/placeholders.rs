//! Placeholders in a paragraph, adapted from Compose's `ParagraphPlaceholderIntegrationTest`, on
//! SkParagraph and on the fixed measurer with the sample font's metrics.

use std::rc::Rc;

use compose::test::basic_measure_font;
use compose::ui::platform::{FixedFontMetrics, FixedTextMeasurer, TextLayoutOptions, TextMeasurer};
use compose::ui::{
    AnnotatedRange, AnnotatedString, Constraints, Density, Paragraph, Placeholder,
    PlaceholderVerticalAlign, Rect, SpanStyle, TextDirection, TextOverflow, TextStyle, TextUnit,
    UnitExt,
};
use compose_skia::SkiaTextMeasurer;

const FONT_SIZE: f32 = 20.0;

fn style() -> TextStyle {
    TextStyle {
        font_size: FONT_SIZE.sp(),
        font_family: Some(basic_measure_font().to_font_family()),
        text_direction: Some(TextDirection::Content),
        ..TextStyle::DEFAULT
    }
}

fn near(actual: f32, expected: f32, what: &str, stack: &str) {
    assert!((actual - expected).abs() < 0.01, "{stack}: {what}: {actual}, not {expected}");
}

fn assert_rect(actual: Rect, expected: Rect, stack: &str) {
    near(actual.left, expected.left, "left", stack);
    near(actual.top, expected.top, "top", stack);
    near(actual.right, expected.right, "right", stack);
    near(actual.bottom, expected.bottom, "bottom", stack);
}

/// A placeholder of 1 em by `height`, aligned as `align`, over bytes `start..end`.
fn over(
    align: PlaceholderVerticalAlign,
    height: TextUnit,
    start: usize,
    end: usize,
) -> AnnotatedRange<Placeholder> {
    AnnotatedRange::new(Placeholder::new(1.0.em(), height, align), start, end)
}

struct Setup {
    placeholders: Vec<AnnotatedRange<Placeholder>>,
    width: Option<f32>,
    height: Option<f32>,
    options: TextLayoutOptions,
}

impl Setup {
    fn with(placeholders: Vec<AnnotatedRange<Placeholder>>) -> Setup {
        Setup { placeholders, width: None, height: None, options: TextLayoutOptions::default() }
    }

    fn laid_out(
        &self,
        measurer: &mut dyn TextMeasurer,
        text: &AnnotatedString,
    ) -> Rc<dyn Paragraph> {
        let width = self.width.map_or(Constraints::INFINITY, |width| width.ceil() as i32);
        let height = self.height.map_or(Constraints::INFINITY, |height| height.ceil() as i32);
        let min_width = if self.width.is_some() { width } else { 0 };
        let handle = measurer.layout_with_placeholders(
            text,
            &style(),
            &self.placeholders,
            self.options,
            Constraints::new(min_width, width, 0, height),
            &Density::new(1.0),
        );
        let paragraph = measurer.paragraph(handle);
        measurer.release(handle);
        paragraph
    }

    fn on_each_stack(
        &self,
        text: impl Into<AnnotatedString>,
        check: impl Fn(&str, &dyn Paragraph),
    ) {
        let text = text.into();
        let mut fixed = FixedTextMeasurer::with_metrics(FixedFontMetrics::SAMPLE_FONT);
        check("the fixed measurer", &*self.laid_out(&mut fixed, &text));
        let mut skia = SkiaTextMeasurer::new();
        check("SkParagraph", &*self.laid_out(&mut skia, &text));
    }
}

/// The one placeholder's box, checked to be there.
fn only_rect(paragraph: &dyn Paragraph, stack: &str) -> Rect {
    let rects = paragraph.placeholder_rects();
    assert_eq!(rects.len(), 1, "{stack}");
    rects[0].unwrap_or_else(|| panic!("{stack}: no rect"))
}

/// `AAA` with a placeholder over its second letter, one character from the left.
fn after_one_character(
    align: PlaceholderVerticalAlign,
    height: TextUnit,
    check: impl Fn(&str, &dyn Paragraph, Rect),
) {
    Setup::with(vec![over(align, height, 1, 2)]).on_each_stack("AAA", |stack, paragraph| {
        let rect = only_rect(paragraph, stack);
        near(rect.left, FONT_SIZE, "left", stack);
        near(rect.right, 2.0 * FONT_SIZE, "right", stack);
        check(stack, paragraph, rect);
    });
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphPlaceholderIntegrationTest.kt, placeHolder_alignAboveBaseLine_lessThanOriginalHeight
#[test]
fn a_short_placeholder_above_the_baseline_leaves_the_line_alone() {
    after_one_character(
        PlaceholderVerticalAlign::AboveBaseline,
        0.5.em(),
        |stack, paragraph, rect| {
            near(paragraph.get_line_height(0), FONT_SIZE, "line", stack);
            near(rect.bottom, paragraph.first_baseline(), "bottom", stack);
            near(rect.top, paragraph.first_baseline() - 0.5 * FONT_SIZE, "top", stack);
        },
    );
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphPlaceholderIntegrationTest.kt, placeHolder_alignAboveBaseLine_greaterThanOriginalHeight
#[test]
fn a_tall_placeholder_above_the_baseline_pushes_the_ascent() {
    after_one_character(
        PlaceholderVerticalAlign::AboveBaseline,
        2.0.em(),
        |stack, paragraph, rect| {
            near(paragraph.get_line_height(0), 0.2 * FONT_SIZE + 2.0 * FONT_SIZE, "line", stack);
            near(rect.bottom, paragraph.first_baseline(), "bottom", stack);
            near(rect.top, paragraph.first_baseline() - 2.0 * FONT_SIZE, "top", stack);
        },
    );
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphPlaceholderIntegrationTest.kt, placeHolder_alignBottom_lessThanOriginalHeight
#[test]
fn a_short_placeholder_at_the_bottom_leaves_the_line_alone() {
    after_one_character(PlaceholderVerticalAlign::Bottom, 0.5.em(), |stack, paragraph, rect| {
        near(paragraph.get_line_height(0), FONT_SIZE, "line", stack);
        near(rect.bottom, paragraph.get_line_bottom(0), "bottom", stack);
        near(rect.top, paragraph.get_line_bottom(0) - 0.5 * FONT_SIZE, "top", stack);
    });
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphPlaceholderIntegrationTest.kt, placeHolder_alignBottom_greaterThanOriginalHeight
#[test]
fn a_tall_placeholder_at_the_bottom_makes_the_line_its_height() {
    after_one_character(PlaceholderVerticalAlign::Bottom, 2.0.em(), |stack, paragraph, rect| {
        near(paragraph.get_line_height(0), 2.0 * FONT_SIZE, "line", stack);
        near(rect.bottom, paragraph.get_line_bottom(0), "bottom", stack);
        near(rect.top, paragraph.get_line_bottom(0) - 2.0 * FONT_SIZE, "top", stack);
    });
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphPlaceholderIntegrationTest.kt, placeHolder_alignTop_lessThanOriginalHeight
#[test]
fn a_short_placeholder_at_the_top_leaves_the_line_alone() {
    after_one_character(PlaceholderVerticalAlign::Top, 0.5.em(), |stack, paragraph, rect| {
        near(paragraph.get_line_height(0), FONT_SIZE, "line", stack);
        near(rect.top, 0.0, "top", stack);
        near(rect.bottom, 0.5 * FONT_SIZE, "bottom", stack);
    });
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphPlaceholderIntegrationTest.kt, placeHolder_alignTop_greaterThanOriginalHeight
#[test]
fn a_tall_placeholder_at_the_top_makes_the_line_its_height() {
    after_one_character(PlaceholderVerticalAlign::Top, 2.0.em(), |stack, paragraph, rect| {
        near(paragraph.get_line_height(0), 2.0 * FONT_SIZE, "line", stack);
        near(rect.top, 0.0, "top", stack);
        near(rect.bottom, 2.0 * FONT_SIZE, "bottom", stack);
    });
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphPlaceholderIntegrationTest.kt, placeHolder_alignCenter_lessThanOriginalHeight
#[test]
fn a_short_placeholder_in_the_center_leaves_the_line_alone() {
    after_one_character(PlaceholderVerticalAlign::Center, 0.5.em(), |stack, paragraph, rect| {
        near(paragraph.get_line_height(0), FONT_SIZE, "line", stack);
        let center = paragraph.get_line_bottom(0) / 2.0;
        near(rect.top, center - 0.25 * FONT_SIZE, "top", stack);
        near(rect.bottom, center + 0.25 * FONT_SIZE, "bottom", stack);
    });
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphPlaceholderIntegrationTest.kt, placeHolder_alignCenter_greaterThanOriginalHeight
#[test]
fn a_tall_placeholder_in_the_center_makes_the_line_its_height() {
    after_one_character(PlaceholderVerticalAlign::Center, 2.0.em(), |stack, paragraph, rect| {
        near(paragraph.get_line_height(0), 2.0 * FONT_SIZE, "line", stack);
        let center = paragraph.get_line_bottom(0) / 2.0;
        near(rect.top, center - FONT_SIZE, "top", stack);
        near(rect.bottom, center + FONT_SIZE, "bottom", stack);
    });
}

/// `AAB` with its `B` larger, so the line is the `B`'s, and a placeholder over the second `A`.
fn beside_a_larger_character(
    align: PlaceholderVerticalAlign,
    check: impl Fn(&str, &dyn Paragraph, Rect),
) {
    let text = AnnotatedString::build(|builder| {
        builder.append("AA");
        let span = SpanStyle { font_size: 30.0.sp(), ..SpanStyle::DEFAULT };
        builder.with_style(span, |builder| {
            builder.append("B");
        });
    });
    Setup::with(vec![over(align, 0.5.em(), 1, 2)]).on_each_stack(text, |stack, paragraph| {
        let rect = only_rect(paragraph, stack);
        near(rect.left, FONT_SIZE, "left", stack);
        near(rect.right, 2.0 * FONT_SIZE, "right", stack);
        check(stack, paragraph, rect);
    });
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphPlaceholderIntegrationTest.kt, placeHolder_alignTextTop
#[test]
fn a_placeholder_at_the_text_top_is_at_the_top_of_the_text_around_it() {
    beside_a_larger_character(PlaceholderVerticalAlign::TextTop, |stack, paragraph, rect| {
        let top = paragraph.first_baseline() - 0.8 * FONT_SIZE;
        near(rect.top, top, "top", stack);
        near(rect.bottom, top + 0.5 * FONT_SIZE, "bottom", stack);
    });
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphPlaceholderIntegrationTest.kt, placeHolder_alignTextBottom
#[test]
fn a_placeholder_at_the_text_bottom_is_at_the_bottom_of_the_text_around_it() {
    beside_a_larger_character(PlaceholderVerticalAlign::TextBottom, |stack, paragraph, rect| {
        let bottom = paragraph.first_baseline() + 0.2 * FONT_SIZE;
        near(rect.top, bottom - 0.5 * FONT_SIZE, "top", stack);
        near(rect.bottom, bottom, "bottom", stack);
    });
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphPlaceholderIntegrationTest.kt, placeHolder_alignTextCenter
#[test]
fn a_placeholder_in_the_text_center_is_in_the_center_of_the_text_around_it() {
    beside_a_larger_character(PlaceholderVerticalAlign::TextCenter, |stack, paragraph, rect| {
        let center = paragraph.first_baseline() - 0.3 * FONT_SIZE;
        near(rect.top, center - 0.25 * FONT_SIZE, "top", stack);
        near(rect.bottom, center + 0.25 * FONT_SIZE, "bottom", stack);
    });
}

/// Above the baseline, a square em over bytes `start..end` of `text`, in a width of its length and
/// three more ems.
fn in_a_wider_paragraph(
    text: &str,
    length: usize,
    start: usize,
    end: usize,
    left: f32,
    right: f32,
) {
    let width = FONT_SIZE * (length + 3) as f32;
    let mut setup =
        Setup::with(vec![over(PlaceholderVerticalAlign::AboveBaseline, 1.0.em(), start, end)]);
    setup.width = Some(width);
    setup.on_each_stack(text.to_string(), |stack, paragraph| {
        assert_eq!(paragraph.line_count(), 1, "{stack}");
        let rect = only_rect(paragraph, stack);
        let bottom = paragraph.first_baseline();
        let expected = Rect::new(
            width - left * FONT_SIZE,
            bottom - FONT_SIZE,
            width - right * FONT_SIZE,
            bottom,
        );
        assert_rect(rect, expected, stack);
    });
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphPlaceholderIntegrationTest.kt, placeHolder_withRtlText
#[test]
fn a_placeholder_right_to_left_counts_from_the_right() {
    // |   RR_R|
    in_a_wider_paragraph("\u{5D0}\u{5D0}\u{5D0}\u{5D0}", 4, 2, 4, 2.0, 1.0);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphPlaceholderIntegrationTest.kt, placeHolder_withBiDiText_coversRtlChar
#[test]
fn a_placeholder_over_a_right_to_left_character_of_mixed_text() {
    // |   LL_R|
    in_a_wider_paragraph("\u{5D0}\u{5D0}AA", 4, 2, 4, 2.0, 1.0);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphPlaceholderIntegrationTest.kt, placeHolder_withBiDiText_coversLtrChar
#[test]
fn a_placeholder_over_a_left_to_right_character_of_mixed_text() {
    // |   L_RR|
    in_a_wider_paragraph("\u{5D0}\u{5D0}AA", 4, 4, 5, 3.0, 2.0);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphPlaceholderIntegrationTest.kt, placeHolder_withBiDiText_coversBiDiStr
#[test]
fn a_placeholder_over_both_directions_of_mixed_text() {
    // |   L_R|, two characters covered.
    in_a_wider_paragraph("\u{5D0}\u{5D0}AA", 4, 2, 5, 2.0, 1.0);
}

fn first_and_third() -> Vec<AnnotatedRange<Placeholder>> {
    let align = PlaceholderVerticalAlign::TextCenter;
    vec![over(align, 1.0.em(), 0, 1), over(align, 1.0.em(), 2, 3)]
}

fn assert_second_is_cut(stack: &str, paragraph: &dyn Paragraph) {
    let rects = paragraph.placeholder_rects();
    assert_eq!(rects.len(), 2, "{stack}");
    assert!(rects[0].is_some(), "{stack}");
    assert!(rects[1].is_none(), "{stack}: {:?}", rects[1]);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphPlaceholderIntegrationTest.kt, placeHolderRects_ellipsized
#[test]
fn an_ellipsized_placeholder_has_no_rect() {
    let mut setup = Setup::with(first_and_third());
    setup.width = Some(2.0 * FONT_SIZE);
    setup.options = TextLayoutOptions {
        max_lines: 1,
        overflow: TextOverflow::Ellipsis,
        ..TextLayoutOptions::default()
    };
    setup.on_each_stack("ABC", assert_second_is_cut);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphPlaceholderIntegrationTest.kt, placeHolderRects_withLimitedHeight_ellipsized
#[test]
fn a_placeholder_ellipsized_by_the_height_has_no_rect() {
    let mut setup = Setup::with(first_and_third());
    setup.width = Some(2.0 * FONT_SIZE);
    setup.height = Some(1.3 * FONT_SIZE);
    setup.options =
        TextLayoutOptions { overflow: TextOverflow::Ellipsis, ..TextLayoutOptions::default() };
    setup.on_each_stack("ABC", assert_second_is_cut);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphPlaceholderIntegrationTest.kt, placeHolderRects_withLimitedMaxLines
#[test]
fn a_placeholder_past_the_maximum_lines_has_no_rect() {
    let mut setup = Setup::with(first_and_third());
    setup.width = Some(2.0 * FONT_SIZE);
    setup.height = Some(FONT_SIZE);
    setup.options = TextLayoutOptions { max_lines: 1, ..TextLayoutOptions::default() };
    setup.on_each_stack("ABC", assert_second_is_cut);
}

#[test]
fn a_placeholder_from_a_start_or_middle_ellipsis_on_has_no_rect() {
    let align = PlaceholderVerticalAlign::Bottom;
    let two = || vec![over(align, 0.5.em(), 0, 1), over(align, 0.5.em(), 4, 5)];
    let ellipsized =
        |overflow| TextLayoutOptions { max_lines: 1, overflow, ..TextLayoutOptions::default() };
    let mut middle = Setup::with(two());
    middle.width = Some(3.0 * FONT_SIZE);
    middle.options = ellipsized(TextOverflow::MiddleEllipsis);
    middle.on_each_stack("abcde", |stack, paragraph| {
        let rects = paragraph.placeholder_rects();
        assert!(rects[0].is_some(), "{stack}");
        assert!(rects[1].is_none(), "{stack}: {:?}", rects[1]);
    });
    let mut start = Setup::with(two());
    start.width = Some(3.0 * FONT_SIZE);
    start.options = ellipsized(TextOverflow::StartEllipsis);
    start.on_each_stack("abcde", |stack, paragraph| {
        assert_eq!(paragraph.placeholder_rects(), vec![None, None], "{stack}");
    });
}

#[test]
fn a_placeholders_offsets_are_the_texts() {
    // Over two letters, what's after it stays where the text says.
    let setup = Setup::with(vec![over(PlaceholderVerticalAlign::Bottom, 0.5.em(), 1, 3)]);
    setup.on_each_stack("ABCD", |stack, paragraph| {
        near(paragraph.get_bounding_box(0).right, FONT_SIZE, "A", stack);
        near(paragraph.get_bounding_box(1).left, FONT_SIZE, "the placeholder", stack);
        near(paragraph.get_bounding_box(1).right, 2.0 * FONT_SIZE, "the placeholder", stack);
        near(paragraph.get_bounding_box(2).left, 2.0 * FONT_SIZE, "hidden", stack);
        near(paragraph.get_bounding_box(2).right, 2.0 * FONT_SIZE, "hidden", stack);
        near(paragraph.get_bounding_box(3).left, 2.0 * FONT_SIZE, "D", stack);
        near(paragraph.get_bounding_box(3).right, 3.0 * FONT_SIZE, "D", stack);
        near(paragraph.max_intrinsic_width(), 3.0 * FONT_SIZE, "own width", stack);
    });
}

#[test]
fn a_placeholder_in_sp_is_that_size_whatever_the_font_size() {
    let placeholder =
        Placeholder::new(30.0.sp(), 10.0.sp(), PlaceholderVerticalAlign::AboveBaseline);
    let setup = Setup::with(vec![AnnotatedRange::new(placeholder, 0, 1)]);
    setup.on_each_stack("A", |stack, paragraph| {
        let rect = only_rect(paragraph, stack);
        near(rect.right - rect.left, 30.0, "width", stack);
        near(rect.bottom - rect.top, 10.0, "height", stack);
    });
}

fn square(
    align: PlaceholderVerticalAlign,
    start: usize,
    end: usize,
) -> AnnotatedRange<Placeholder> {
    over(align, 1.0.em(), start, end)
}

#[test]
fn placeholders_out_of_order_keep_theirs() {
    let align = PlaceholderVerticalAlign::Bottom;
    let setup = Setup::with(vec![square(align, 5, 6), square(align, 1, 2)]);
    setup.on_each_stack("abcdefgh", |stack, paragraph| {
        let rects = paragraph.placeholder_rects();
        near(rects[0].expect("the first").left, 5.0 * FONT_SIZE, "the first given", stack);
        near(rects[1].expect("the second").left, FONT_SIZE, "the second given", stack);
    });
}

#[test]
fn a_placeholder_starting_inside_another_is_left_out() {
    let align = PlaceholderVerticalAlign::Bottom;
    let setup = Setup::with(vec![square(align, 1, 3), square(align, 2, 4)]);
    setup.on_each_stack("abcdefgh", |stack, paragraph| {
        let rects = paragraph.placeholder_rects();
        near(rects[0].expect("the first").left, FONT_SIZE, "the first", stack);
        assert!(rects[1].is_none(), "{stack}");
        near(paragraph.max_intrinsic_width(), 7.0 * FONT_SIZE, "own width", stack);
    });
}

#[test]
fn a_run_ending_inside_a_placeholder_adds_it_once() {
    // A span over `ab`, a placeholder over `bc`.
    let text = AnnotatedString::build(|builder| {
        let span = SpanStyle { letter_spacing: 0.0.sp(), ..SpanStyle::DEFAULT };
        builder.with_style(span, |builder| {
            builder.append("ab");
        });
        builder.append("cd");
    });
    let setup = Setup::with(vec![square(PlaceholderVerticalAlign::Bottom, 1, 3)]);
    setup.on_each_stack(text, |stack, paragraph| {
        // `a`, the placeholder, `d`.
        near(paragraph.max_intrinsic_width(), 3.0 * FONT_SIZE, "own width", stack);
        near(paragraph.get_bounding_box(3).left, 2.0 * FONT_SIZE, "d", stack);
    });
}

#[test]
fn a_placeholder_a_start_ellipsis_hides_takes_no_room() {
    let mut setup = Setup::with(vec![square(PlaceholderVerticalAlign::Bottom, 1, 2)]);
    setup.width = Some(3.0 * FONT_SIZE);
    setup.options = TextLayoutOptions {
        max_lines: 1,
        overflow: TextOverflow::StartEllipsis,
        ..TextLayoutOptions::default()
    };
    setup.on_each_stack("abcde", |stack, paragraph| {
        assert_eq!(paragraph.placeholder_rects(), vec![None], "{stack}");
        near(paragraph.get_line_width(0), 3.0 * FONT_SIZE, "line", stack);
        near(paragraph.get_bounding_box(4).right, 3.0 * FONT_SIZE, "e", stack);
    });
}

#[test]
fn a_placeholder_over_the_first_strong_character_does_not_set_the_direction() {
    let setup = Setup::with(vec![square(PlaceholderVerticalAlign::Bottom, 0, 2)]);
    setup.on_each_stack("\u{5D0}abc", |stack, paragraph| {
        assert_eq!(
            paragraph.get_paragraph_direction(0),
            compose::ui::ResolvedTextDirection::Ltr,
            "{stack}"
        );
        near(only_rect(paragraph, stack).left, 0.0, "left", stack);
    });
}

#[test]
fn a_tall_placeholder_moves_the_baseline_as_its_alignment_says() {
    use PlaceholderVerticalAlign as Align;
    for (align, baseline) in [
        (Align::AboveBaseline, 2.0),
        (Align::Top, 0.8),
        (Align::Bottom, 1.8),
        (Align::Center, 1.3),
        (Align::TextTop, 0.8),
        (Align::TextBottom, 1.8),
        (Align::TextCenter, 1.3),
    ] {
        after_one_character(align, 2.0.em(), |stack, paragraph, _| {
            near(paragraph.first_baseline(), baseline * FONT_SIZE, "baseline", stack);
        });
    }
}

/// Where SkParagraph itself left each placeholder of `text`, left to right.
fn placed_by_skia(text: &AnnotatedString, setup: &Setup) -> (Vec<Rect>, Vec<Option<Rect>>) {
    let mut skia = SkiaTextMeasurer::new();
    let handle = skia.layout_with_placeholders(
        text,
        &style(),
        &setup.placeholders,
        setup.options,
        Constraints::new(0, setup.width.unwrap_or(1000.0) as i32, 0, Constraints::INFINITY),
        &Density::new(1.0),
    );
    let placed = skia
        .skia_paragraph(handle)
        .expect("a live paragraph")
        .get_rects_for_placeholders()
        .iter()
        .map(|text_box| {
            let rect = text_box.rect;
            Rect::new(rect.left, rect.top, rect.right, rect.bottom)
        })
        .collect();
    let answered = skia.paragraph(handle).placeholder_rects();
    skia.release(handle);
    (placed, answered)
}

#[test]
fn a_placeholders_rect_is_where_skparagraph_left_it() {
    use PlaceholderVerticalAlign as Align;
    let plain = AnnotatedString::from("AAA");
    for align in [
        Align::AboveBaseline,
        Align::Top,
        Align::Bottom,
        Align::Center,
        Align::TextTop,
        Align::TextBottom,
        Align::TextCenter,
    ] {
        for height in [0.5, 0.75, 1.0, 2.0] {
            let setup = Setup::with(vec![over(align, height.em(), 1, 2)]);
            let (placed, answered) = placed_by_skia(&plain, &setup);
            assert_rect(answered[0].expect("a rect"), placed[0], &format!("{align:?} {height}"));
        }
    }
}

#[test]
fn beside_a_larger_character_the_text_s_alignments_are_where_skparagraph_left_them() {
    use PlaceholderVerticalAlign as Align;
    let beside = AnnotatedString::build(|builder| {
        builder.append("AA");
        let span = SpanStyle { font_size: 30.0.sp(), ..SpanStyle::DEFAULT };
        builder.with_style(span, |builder| {
            builder.append("B");
        });
    });
    // A line's own alignments are about its whole height in Compose's rects, and about the font
    // around them in SkParagraph's, which draws nothing there (D1127).
    for align in [Align::AboveBaseline, Align::TextTop, Align::TextBottom, Align::TextCenter] {
        for height in [0.5, 0.75, 1.0] {
            let setup = Setup::with(vec![over(align, height.em(), 1, 2)]);
            let (placed, answered) = placed_by_skia(&beside, &setup);
            assert_rect(answered[0].expect("a rect"), placed[0], &format!("{align:?} {height}"));
        }
    }
}

#[test]
fn adjacent_placeholders_right_to_left_are_where_skparagraph_left_them() {
    let align = PlaceholderVerticalAlign::Bottom;
    let mut setup = Setup::with(vec![square(align, 2, 4), square(align, 4, 6)]);
    setup.width = Some(5.0 * FONT_SIZE);
    let (mut placed, answered) =
        placed_by_skia(&AnnotatedString::from("\u{5D0}\u{5D0}\u{5D0}\u{5D0}"), &setup);
    let mut lefts: Vec<f32> = answered.iter().map(|rect| rect.expect("a rect").left).collect();
    lefts.sort_by(f32::total_cmp);
    placed.sort_by(|first, second| first.left.total_cmp(&second.left));
    near(lefts[0], placed[0].left, "the first's", "SkParagraph");
    near(lefts[1], placed[1].left, "the second's", "SkParagraph");
}

const HELLOS: &str = "Hello World Hello World Hello World\u{FFFD}";

fn at_the_end_of_hellos() -> Setup {
    let placeholder =
        Placeholder::new(10.0.sp(), 10.0.sp(), PlaceholderVerticalAlign::AboveBaseline);
    let start = HELLOS.len() - '\u{FFFD}'.len_utf8();
    Setup::with(vec![AnnotatedRange::new(placeholder, start, HELLOS.len())])
}

/// `setup`'s text at 20 pixels' font as 10 pixels' would be: at `ems` of the 20 em wide.
fn two_lines(mut setup: Setup, ems: f32, overflow: TextOverflow) -> Setup {
    setup.width = Some(ems * FONT_SIZE);
    setup.options = TextLayoutOptions { max_lines: 2, overflow, ..TextLayoutOptions::default() };
    setup
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, placeholder_onEllipsizedLastLine_isNotNull
#[test]
fn a_placeholder_on_a_last_line_with_room_has_a_rect() {
    two_lines(at_the_end_of_hellos(), 20.0, TextOverflow::Ellipsis)
        .on_each_stack(HELLOS.to_string(), |stack, paragraph| {
            assert!(only_rect_or_none(paragraph, stack).is_some(), "{stack}")
        });
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, placeholder_fullyEllipsizedAway_isNull
#[test]
fn a_placeholder_ellipsized_away_has_no_rect() {
    two_lines(at_the_end_of_hellos(), 15.0, TextOverflow::Ellipsis)
        .on_each_stack(HELLOS.to_string(), |stack, paragraph| {
            assert!(only_rect_or_none(paragraph, stack).is_none(), "{stack}")
        });
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, placeholder_onLastLine_withOverflowClip_isNotNull
#[test]
fn a_clipped_placeholder_on_a_last_line_with_room_has_a_rect() {
    two_lines(at_the_end_of_hellos(), 20.0, TextOverflow::Clip)
        .on_each_stack(HELLOS.to_string(), |stack, paragraph| {
            assert!(only_rect_or_none(paragraph, stack).is_some(), "{stack}")
        });
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/AndroidParagraphTest.kt, placeholder_onNonEllipsizedLine_isNotNull
#[test]
fn a_placeholder_before_the_ellipsis_has_a_rect() {
    let before = "Hello ";
    let text = format!("{before}\u{FFFD} World Hello World Hello World");
    let placeholder =
        Placeholder::new(10.0.sp(), 10.0.sp(), PlaceholderVerticalAlign::AboveBaseline);
    let range =
        AnnotatedRange::new(placeholder, before.len(), before.len() + '\u{FFFD}'.len_utf8());
    two_lines(Setup::with(vec![range]), 15.0, TextOverflow::Ellipsis)
        .on_each_stack(text, |stack, paragraph| {
            assert!(only_rect_or_none(paragraph, stack).is_some(), "{stack}")
        });
}

fn only_rect_or_none(paragraph: &dyn Paragraph, stack: &str) -> Option<Rect> {
    let rects = paragraph.placeholder_rects();
    assert_eq!(rects.len(), 1, "{stack}");
    rects[0]
}
