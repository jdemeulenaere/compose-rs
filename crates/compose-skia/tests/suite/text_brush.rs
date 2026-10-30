//! What a text is painted with on the real stack, adapted from Compose's `TextPainterTest` and
//! `ParagraphIntegrationTest`: gradients, shadows and strokes, compared picture to picture and,
//! in the sample font's triangles, read at pixels.

use compose::foundation::layout::*;
use compose::foundation::*;
use compose::test::basic_measure_font;
use compose::ui::platform::UiHost;
use compose::ui::{
    AnnotatedString, Brush, Color, Constraints, Density, DrawStyle, DrawTextLayoutOptions,
    DrawTextOptions, IntSize, Modifier, Offset, Shadow, SpanStyle, Stroke, StrokeCap,
    TextDirection, TextLayoutResult, TextMeasureOptions, TextMeasurer, TextStyle, UnitExt,
};
use compose_skia::SkiaTextMeasurer;

use super::text_painter::{LONG_TEXT, Picture, draw_with, render};

fn sample(font_size: f32) -> TextStyle {
    TextStyle {
        font_family: Some(basic_measure_font().to_font_family()),
        font_size: font_size.sp(),
        text_direction: Some(TextDirection::Content),
        ..TextStyle::DEFAULT
    }
}

fn red_to_blue() -> Brush {
    Brush::radial_gradient([Color::RED, Color::BLUE])
}

fn green_to_yellow() -> Brush {
    Brush::radial_gradient([Color::GREEN, Color::YELLOW])
}

fn left_to_right() -> Brush {
    Brush::horizontal_gradient([Color::RED, Color::BLUE])
}

/// `LONG_TEXT` in the sample font at 20 pixels, within a square of 400.
fn measured(
    measurer: &TextMeasurer,
    style: TextStyle,
    constraints: Constraints,
) -> TextLayoutResult {
    measurer.measure(
        LONG_TEXT,
        TextMeasureOptions { style, constraints, ..TextMeasureOptions::default() },
    )
}

fn drawn(block: impl Fn(&mut compose::ui::DrawScope<'_>, &TextMeasurer) + 'static) -> Picture {
    draw_with((400, 400), (400, 400), 1.0, 0, block)
}

/// `text` in `style`, drawn in a picture of `width` by `height`.
fn picture(
    text: impl Into<AnnotatedString> + Clone + 'static,
    style: TextStyle,
    width: i32,
    height: i32,
) -> Picture {
    draw_with((width, height), (width, height), 1.0, 0, move |scope, measurer| {
        let options = DrawTextOptions { style: style.clone(), ..DrawTextOptions::default() };
        scope.draw_text(measurer, text.clone(), options);
    })
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextPainterTest.kt, drawTextLayout_shouldChangeBrush
#[test]
fn a_layout_is_drawn_with_the_brush_given() {
    let constraints = Constraints::fixed(400, 400);
    let first = drawn(move |scope, measurer| {
        let style = sample(20.0).with_brush(Some(red_to_blue()), f32::NAN);
        let result = measured(measurer, style, constraints);
        scope.draw_text_layout_with_brush(
            &result,
            green_to_yellow(),
            DrawTextLayoutOptions::default(),
        );
    });
    let second = drawn(move |scope, measurer| {
        let style = sample(20.0).with_brush(Some(green_to_yellow()), f32::NAN);
        let result = measured(measurer, style, constraints);
        scope.draw_text_layout(&result, Color::UNSPECIFIED, DrawTextLayoutOptions::default());
    });
    assert!(!first.is_blank());
    assert_eq!(first, second);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextPainterTest.kt, drawTextLayout_shouldChangeAlphaForBrush
#[test]
fn a_layout_is_drawn_at_the_alpha_given_over_its_brush() {
    let constraints = Constraints::fixed(400, 400);
    let first = drawn(move |scope, measurer| {
        let style = sample(20.0).with_brush(Some(red_to_blue()), 1.0);
        let result = measured(measurer, style, constraints);
        let options = DrawTextLayoutOptions { alpha: 0.5, ..DrawTextLayoutOptions::default() };
        scope.draw_text_layout(&result, Color::UNSPECIFIED, options);
    });
    let second = drawn(move |scope, measurer| {
        let style = sample(20.0).with_brush(Some(red_to_blue()), 0.5);
        let result = measured(measurer, style, constraints);
        scope.draw_text_layout(&result, Color::UNSPECIFIED, DrawTextLayoutOptions::default());
    });
    assert!(!first.is_blank());
    assert_eq!(first, second);
}

fn round_stroke() -> DrawStyle {
    DrawStyle::Stroke(Stroke { cap: StrokeCap::Round, ..Stroke::new(8.0) })
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextPainterTest.kt, drawTextLayout_shouldChangeDrawStyle
#[test]
fn a_layout_is_drawn_in_the_draw_style_given() {
    let constraints = Constraints::new(0, 400, 0, 400);
    let first = drawn(move |scope, measurer| {
        let style = TextStyle { draw_style: Some(DrawStyle::Fill), ..sample(20.0) };
        let result = measured(measurer, style, constraints);
        let options = DrawTextLayoutOptions {
            draw_style: Some(round_stroke()),
            ..DrawTextLayoutOptions::default()
        };
        scope.draw_text_layout(&result, Color::UNSPECIFIED, options);
    });
    let second = drawn(move |scope, measurer| {
        let style = TextStyle { draw_style: Some(round_stroke()), ..sample(20.0) };
        let result = measured(measurer, style, constraints);
        scope.draw_text_layout(&result, Color::UNSPECIFIED, DrawTextLayoutOptions::default());
    });
    assert!(!first.is_blank());
    assert_eq!(first, second);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testDefaultSpanStyle_setBrush
#[test]
fn a_brush_changes_what_a_text_looks_like() {
    let style = sample(100.0);
    let brushed = style.clone().with_brush(Some(left_to_right()), f32::NAN);
    assert_ne!(picture("abc", style, 300, 100), picture("abc", brushed, 300, 100));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testDefaultSpanStyle_setBrushAlpha
#[test]
fn a_brushs_alpha_changes_what_a_text_looks_like() {
    let opaque = sample(100.0).with_brush(Some(left_to_right()), f32::NAN);
    let faded = sample(100.0).with_brush(Some(left_to_right()), 0.5);
    assert_ne!(picture("abc", opaque, 300, 100), picture("abc", faded, 300, 100));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testDefaultSpanStyle_overrideAlphaDuringDraw
#[test]
fn an_alpha_given_when_drawing_is_the_one_a_style_would_have() {
    let first = draw_with((300, 100), (300, 100), 1.0, 0, |scope, measurer| {
        let style = sample(100.0).with_brush(Some(left_to_right()), f32::NAN);
        let options = TextMeasureOptions { style, ..TextMeasureOptions::default() };
        let result = measurer.measure("abc", options);
        let options = DrawTextLayoutOptions { alpha: 0.5, ..DrawTextLayoutOptions::default() };
        scope.draw_text_layout_with_brush(&result, left_to_right(), options);
    });
    let faded = sample(100.0).with_brush(Some(left_to_right()), 0.5);
    assert_eq!(first, picture("abc", faded, 300, 100));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testSolidBrushColorIsSameAsColor
#[test]
fn a_solid_brush_draws_as_its_color() {
    let colored = TextStyle { color: Color::RED, ..sample(100.0) };
    let solid = sample(100.0).with_brush(Some(Brush::solid(Color::RED)), f32::NAN);
    assert_eq!(picture("abc", colored, 300, 100), picture("abc", solid, 300, 100));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testSpanBrush_overridesDefaultBrush
#[test]
fn a_spans_brush_overrides_the_styles() {
    let style = sample(100.0).with_brush(Some(left_to_right()), f32::NAN);
    let spanned = AnnotatedString::build(|builder| {
        let span = SpanStyle {
            brush: Some(Brush::horizontal_gradient([Color::YELLOW, Color::GREEN])),
            ..SpanStyle::DEFAULT
        };
        builder.with_style(span, |builder| {
            builder.append("abc");
        });
    });
    let plain = picture("abc", style.clone(), 300, 100);
    let over = picture(spanned, style, 300, 100);
    assert_ne!(plain, over);
    // The span's own gradient: yellow at its left, green at its right.
    let at_left = over.pixel(5, 50);
    assert!(at_left.red() > 200 && at_left.green() > 200, "{at_left:?}");
    let at_right = over.pixel(290, 50);
    assert!(at_right.red() < 60 && at_right.green() > 100, "{at_right:?}");
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testBrush_notEffectedBy_TextDirection
#[test]
fn a_brush_is_the_same_whichever_way_a_text_runs() {
    let brush = Brush::linear_gradient([Color::RED, Color::BLUE]);
    let style = sample(100.0).with_brush(Some(brush), f32::NAN);
    let left_to_right = picture("aaa", style.clone(), 300, 100);
    let right_to_left = picture("\u{5D0}\u{5D0}\u{5D0}", style, 300, 100);
    assert_ne!(left_to_right, right_to_left);
    assert_eq!(left_to_right.pixel(50, 50), right_to_left.pixel(50, 50));
}

#[test]
fn a_start_ellipsis_draws_the_ellipsis_glyph_then_the_lines_end() {
    let drawn = draw_with((150, 100), (150, 100), 1.0, 0, |scope, measurer| {
        let options = DrawTextOptions {
            style: sample(50.0),
            overflow: compose::ui::TextOverflow::StartEllipsis,
            max_lines: 1,
            ..DrawTextOptions::default()
        };
        scope.draw_text(measurer, "abcde", options);
    });
    // The sample font's "…" is a bar under the baseline, 40 pixels down; a letter a triangle
    // from the top of the line, with nothing beyond where the line ends.
    assert_ne!(drawn.pixel(2, 45), Color::WHITE);
    assert_eq!(drawn.pixel(2, 5), Color::WHITE);
    assert_ne!(drawn.pixel(52, 5), Color::WHITE);
    assert_ne!(drawn.pixel(102, 5), Color::WHITE);
    assert_eq!(drawn.pixel(52, 60), Color::WHITE);
}

#[test]
fn a_gradient_runs_across_the_paragraph_not_a_glyph() {
    let style = sample(100.0).with_brush(Some(left_to_right()), f32::NAN);
    let drawn = picture("abc", style, 300, 100);
    let at_left = drawn.pixel(5, 50);
    assert!(at_left.red() > 200 && at_left.blue() < 60, "{at_left:?}");
    let in_the_middle = drawn.pixel(150, 50);
    assert!(in_the_middle.red() > 90 && in_the_middle.blue() > 90, "{in_the_middle:?}");
    let at_right = drawn.pixel(290, 50);
    assert!(at_right.blue() > 200 && at_right.red() < 60, "{at_right:?}");
}

#[test]
fn a_gradient_stays_with_a_text_drawn_elsewhere() {
    let style = sample(100.0).with_brush(Some(left_to_right()), f32::NAN);
    let at_origin = picture("abc", style.clone(), 300, 100);
    let moved = draw_with((400, 200), (400, 200), 1.0, 0, move |scope, measurer| {
        let options = DrawTextOptions {
            style: style.clone(),
            top_left: Offset::new(100.0, 100.0),
            ..DrawTextOptions::default()
        };
        scope.draw_text(measurer, "abc", options);
    });
    for (x, y) in [(5, 50), (150, 50), (290, 50)] {
        assert_eq!(at_origin.pixel(x, y), moved.pixel(x + 100, y + 100), "at {x}, {y}");
    }
}

#[test]
fn a_gradient_stays_with_a_text_node_placed_elsewhere() {
    let style = sample(100.0).with_brush(Some(left_to_right()), f32::NAN);
    let at_origin = picture("abc", style.clone(), 300, 100);
    let mut host =
        UiHost::new(IntSize::new(500, 300), Density::new(1.0), Box::new(SkiaTextMeasurer::new()));
    host.set_content(move || {
        let style = style.clone();
        Box!(modifier = Modifier.padding((100.dp(), 100.dp())), {
            BasicText!("abc", style = style.clone());
        });
    });
    host.frame();
    let moved = render(&mut host, (500, 300));
    for (x, y) in [(5, 50), (150, 50), (290, 50)] {
        assert_eq!(at_origin.pixel(x, y), moved.pixel(x + 100, y + 100), "at {x}, {y}");
    }
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, paint_withDrawStyle_changesVisual
#[test]
fn a_stroke_changes_what_a_text_looks_like() {
    let filled = TextStyle { color: Color::RED, ..sample(100.0) };
    let stroked =
        TextStyle { draw_style: Some(DrawStyle::Stroke(Stroke::new(0.0))), ..filled.clone() };
    assert_ne!(picture("aaa", filled, 300, 100), picture("aaa", stroked, 300, 100));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, paint_withDrawStyle_doesNotResetWithNull
// A paragraph there remembers the draw style it was last painted in; a layout here is painted from
// its style, so what stays is the style's.
#[test]
fn a_stroked_layout_drawn_again_without_a_draw_style_stays_stroked() {
    let stroked = TextStyle {
        color: Color::RED,
        draw_style: Some(DrawStyle::Stroke(Stroke::new(4.0))),
        ..sample(100.0)
    };
    let first = picture("aaa", stroked.clone(), 300, 100);
    let second = draw_with((300, 100), (300, 100), 1.0, 0, move |scope, measurer| {
        let result = measurer.measure(
            "aaa",
            TextMeasureOptions { style: stroked.clone(), ..TextMeasureOptions::default() },
        );
        let options =
            DrawTextLayoutOptions { draw_style: None, ..DrawTextLayoutOptions::default() };
        scope.draw_text_layout(&result, Color::UNSPECIFIED, options);
    });
    assert_eq!(first, second);
}

#[test]
fn a_stroke_leaves_the_inside_of_a_glyph_empty() {
    let stroked = TextStyle {
        color: Color::BLACK,
        draw_style: Some(DrawStyle::Stroke(Stroke::new(4.0))),
        ..sample(100.0)
    };
    let drawn = picture("a", stroked, 100, 100);
    assert_eq!(drawn.pixel(30, 50), Color::WHITE);
    // The edge of the triangle's left side.
    assert_ne!(drawn.pixel(1, 50), Color::WHITE);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/ParagraphIntegrationTest.kt, testAnnotatedString_shadow
#[test]
fn a_spans_shadow_changes_what_a_text_looks_like() {
    let shadow = Shadow { color: Color::GREEN, offset: Offset::new(1.0, 2.0), blur_radius: 3.0 };
    let spanned = AnnotatedString::build(|builder| {
        let span = SpanStyle { shadow: Some(shadow), ..SpanStyle::DEFAULT };
        builder.with_style(span, |builder| {
            builder.append("abcde");
        });
    });
    assert_ne!(picture("abcde", sample(20.0), 100, 20), picture(spanned, sample(20.0), 100, 20));
}

#[test]
fn a_shadow_is_drawn_at_its_offset_under_the_text() {
    let shadow = Shadow { color: Color::GREEN, offset: Offset::new(0.0, 30.0), blur_radius: 0.0 };
    let style = TextStyle { color: Color::RED, shadow: Some(shadow), ..sample(50.0) };
    let drawn = picture("a", style, 100, 100);
    // Above the glyph's bottom left, the glyph; below it, its shadow alone.
    assert_eq!(drawn.pixel(2, 25), Color::RED);
    assert_eq!(drawn.pixel(2, 60), Color::GREEN);
}
