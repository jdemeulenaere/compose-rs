//! Review of `port-android-paragraph` (D2180, D2181): a text style's background and a shadow of
//! no blur on SkParagraph, in the cases Compose's Android paragraph decides that the port's
//! tests leave out. Compose puts a text style's background in a `BackgroundColorSpan` before the
//! spans' (`AndroidParagraphIntrinsics`, `generateFallbackSpanStyle`); Android's `TextLine` lets
//! the last span covering a character set its `bgColor`, and draws none for 0.

use std::rc::Rc;

use compose::test::basic_measure_font;
use compose::ui::{
    AnnotatedRange, BlendMode, Color, Constraints, Density, DrawScope, LOCAL_FONT_FAMILY_RESOLVER,
    LocaleList, Offset, Paragraph, Placeholder, PlaceholderVerticalAlign, Shadow, SpanStyle,
    TextDirection, TextOverflow, TextStyle, UnitExt, paragraph_of_text,
};

use super::text_painter::{Picture, draw_with};

const BLUE: Color = Color::from_argb(0xFF00_00FF);
const GREEN: Color = Color::from_argb(0xFF00_FF00);

/// The sample font at 40 px: each Latin letter a triangle 40 px wide pointing right, so (x + 35,
/// 4) of a glyph at x is outside it, where its background shows.
fn style() -> TextStyle {
    TextStyle {
        font_size: 40.0.sp(),
        font_family: Some(basic_measure_font().to_font_family()),
        text_direction: Some(TextDirection::Content),
        ..TextStyle::DEFAULT
    }
}

fn laid_out(
    scope: &DrawScope<'_>,
    text: &str,
    style: &TextStyle,
    spans: &[AnnotatedRange<SpanStyle>],
    placeholders: &[AnnotatedRange<Placeholder>],
) -> Rc<dyn Paragraph> {
    paragraph_of_text(
        text,
        style,
        Constraints::new(0, 200, 0, Constraints::INFINITY),
        Density::new(1.0),
        scope.current_value_of(&LOCAL_FONT_FAMILY_RESOLVER),
        LocaleList::from_language_tags("en"),
        spans,
        placeholders,
        u32::MAX,
        TextOverflow::Clip,
    )
}

fn painted(
    text: &'static str,
    style: TextStyle,
    spans: Vec<AnnotatedRange<SpanStyle>>,
    placeholders: Vec<AnnotatedRange<Placeholder>>,
    paint: impl Fn(&dyn Paragraph, &mut DrawScope<'_>) + 'static,
) -> Picture {
    draw_with((200, 40), (200, 40), 1.0, 0, move |scope, _| {
        let paragraph = laid_out(scope, text, &style, &spans, &placeholders);
        paint(&*paragraph, scope);
    })
}

fn default_paint(
    text: &'static str,
    style: TextStyle,
    spans: Vec<AnnotatedRange<SpanStyle>>,
) -> Picture {
    painted(text, style, spans, vec![], |paragraph, scope| {
        paragraph.paint(scope, Color::UNSPECIFIED, None, None, None, BlendMode::SrcOver);
    })
}

fn background(color: Color, start: usize, end: usize) -> AnnotatedRange<SpanStyle> {
    AnnotatedRange::new(SpanStyle { background: color, ..SpanStyle::DEFAULT }, start, end)
}

#[test]
fn a_span_s_background_replaces_the_text_style_s_on_its_range_only() {
    let picture = default_paint(
        "abcde",
        TextStyle { background: BLUE, ..style() },
        vec![background(GREEN, 0, 3)],
    );
    assert_eq!(picture.pixel(35, 4), GREEN);
    assert_eq!(picture.pixel(115, 4), GREEN);
    assert_eq!(picture.pixel(155, 4), BLUE);
    assert_eq!(picture.pixel(195, 4), BLUE);
}

#[test]
fn a_transparent_span_background_clears_the_text_style_s_on_its_range() {
    // Compose sets a `BackgroundColorSpan(0)` for a specified transparent background: Android's
    // `bgColor` is then 0 there, and no background is drawn.
    let picture = default_paint(
        "abcde",
        TextStyle { background: BLUE, ..style() },
        vec![background(Color::TRANSPARENT, 0, 3)],
    );
    assert_eq!(picture.pixel(35, 4), Color::WHITE);
    assert_eq!(picture.pixel(155, 4), BLUE);
}

#[test]
fn a_text_style_s_translucent_background_under_a_span_s_is_not_drawn_twice() {
    // Android draws one `bgColor` per character, the last span's: not the text style's under it.
    let translucent = Color::from_argb(0x8000_00FF);
    let over = default_paint(
        "abcde",
        TextStyle { background: translucent, ..style() },
        vec![background(translucent, 0, 3)],
    );
    let alone = default_paint("abcde", TextStyle { background: translucent, ..style() }, vec![]);
    assert_eq!(over, alone);
}

#[test]
fn a_text_style_s_background_stays_when_painted_in_another_color_or_shadow() {
    let blue_bg = TextStyle { background: BLUE, ..style() };
    let shadow = Shadow { color: Color::RED, offset: Offset::new(1.0, 1.0), blur_radius: 0.0 };
    let repainted = painted("abcde", blue_bg, vec![], vec![], move |paragraph, scope| {
        paragraph.paint(scope, Color::GREEN, Some(shadow), None, None, BlendMode::SrcOver);
    });
    assert_eq!(repainted.pixel(35, 4), BLUE);
    assert_eq!(repainted.pixel(10, 20), Color::GREEN);
}

#[test]
fn an_empty_text_with_a_background_paints_nothing() {
    let picture = default_paint("", TextStyle { background: BLUE, ..style() }, vec![]);
    assert!(picture.is_blank());
}

#[test]
fn a_placeholder_shows_no_background_as_android_s_replacement_span() {
    // Half an em above the baseline (0.8 em): the box is 40..80 by 12..32, the line unchanged.
    let placeholder = AnnotatedRange::new(
        Placeholder::new(1.0.em(), 0.5.em(), PlaceholderVerticalAlign::AboveBaseline),
        1,
        2,
    );
    let picture = painted(
        "abc",
        TextStyle { background: BLUE, ..style() },
        vec![],
        vec![placeholder.clone()],
        |paragraph, scope| {
            paragraph.paint(scope, Color::UNSPECIFIED, None, None, None, BlendMode::SrcOver);
        },
    );
    assert_eq!(picture.pixel(35, 4), BLUE);
    assert_eq!(picture.pixel(60, 20), Color::WHITE, "inside the placeholder's box");
    assert_eq!(picture.pixel(115, 4), BLUE);

    let spanned = painted(
        "abc",
        style(),
        vec![background(GREEN, 0, 3)],
        vec![placeholder],
        |paragraph, scope| {
            paragraph.paint(scope, Color::UNSPECIFIED, None, None, None, BlendMode::SrcOver);
        },
    );
    assert_eq!(spanned.pixel(35, 4), GREEN);
    assert_eq!(spanned.pixel(60, 20), Color::WHITE, "inside the placeholder's box, spanned");
    assert_eq!(spanned.pixel(115, 4), GREEN);
}

#[test]
#[ignore = "review: SkParagraph's background covers a run's own ascent to descent; Android's TextLine fills the line's top to bottom, so a smaller run beside a larger one has a shorter background on the desktop (span backgrounds too, before D2180)"]
fn a_background_fills_the_whole_line_beside_a_larger_span() {
    let larger = AnnotatedRange::new(SpanStyle { font_size: 40.sp(), ..SpanStyle::DEFAULT }, 0, 1);
    let style = TextStyle { font_size: 20.0.sp(), background: BLUE, ..style() };
    let picture = default_paint("abcde", style, vec![larger]);
    // "b", 20 px, at 40..60 on a line 40 px high whose baseline is the 40 px "a"'s, at 32.
    assert_eq!(picture.pixel(58, 2), BLUE);
}

#[test]
fn a_span_s_shadow_of_none_clears_the_text_style_s_on_its_range() {
    // Compose's `ShadowSpan` of `Shadow.None` sets a transparent shadow layer over the paint's.
    let down = Shadow { color: GREEN, offset: Offset::new(0.0, 18.0), blur_radius: 0.0 };
    let none =
        AnnotatedRange::new(SpanStyle { shadow: Some(Shadow::NONE), ..SpanStyle::DEFAULT }, 0, 3);
    let style = TextStyle { font_size: 20.0.sp(), shadow: Some(down), ..style() };
    let picture = default_paint("abcde", style, vec![none]);
    assert_eq!(picture.pixel(2, 28), Color::WHITE);
    assert_ne!(picture.pixel(62, 28), Color::WHITE);
}

#[test]
fn painting_with_a_shadow_of_no_blur_draws_it_at_the_least_blur() {
    let zero = Shadow { color: GREEN, offset: Offset::new(1.0, 2.0), blur_radius: 0.0 };
    let least = Shadow { blur_radius: f32::from_bits(1), ..zero };
    let style = TextStyle { font_size: 20.0.sp(), ..style() };
    let painted_with = |shadow: Shadow| {
        painted("abc", style.clone(), vec![], vec![], move |paragraph, scope| {
            paragraph.paint(
                scope,
                Color::UNSPECIFIED,
                Some(shadow),
                None,
                None,
                BlendMode::SrcOver,
            );
        })
    };
    let laid_out_with =
        default_paint("abc", TextStyle { shadow: Some(least), ..style.clone() }, vec![]);
    assert_eq!(painted_with(zero), laid_out_with);
    assert_eq!(painted_with(zero), painted_with(least));
    assert_ne!(painted_with(zero), default_paint("abc", style.clone(), vec![]));
}
