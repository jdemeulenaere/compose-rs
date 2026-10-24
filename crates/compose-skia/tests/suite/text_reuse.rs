//! D551's optimization with real text (SkParagraph, the system's fonts): a host reusing a
//! paragraph whose layout width doesn't change draws each frame exactly as a host following
//! Compose's rule, line breaks, alignment, ellipsis and right-to-left text included, while its
//! container's width changes every frame.

#![allow(non_snake_case)]

use compose::foundation::layout::*;
use compose::foundation::*;
use compose::runtime::*;
use compose::test::*;
use compose::ui::{
    AnnotatedString, Constraints, Density, IntSize, Modifier, SpanStyle, TextAlign, TextOverflow,
    TextStyle, UnitExt, platform::UiHost,
};
use compose_skia::SkiaTextMeasurer;

const SHORT: &str = "Short label";
const CENTERED: &str = "Centered";
const CENTERED_WIDE: &str = "Centered, whole width";
const END_WIDE: &str = "End, whole width";
const WRAPPING: &str =
    "A sentence long enough to wrap onto a second line at every width the test gives it here.";
const ELLIPSIZED: &str =
    "A single line long enough to be ellipsized at every width the test gives it, always.";
const UNWRAPPED: &str = "Never wrapped, whatever the width";
const TWO_LINES: &str = "First line\nSecond";
const HALF_MIN: &str = "Half min";
const HEBREW: &str = "שלום עולם";
const HEBREW_WIDE: &str = "שלום עולם, כל הרוחב";
const ARABIC_END: &str = "مرحبا بالعالم";
const JUSTIFIED: &str =
    "Justified text, long enough to wrap onto two or three lines at every width given here.";
const SPACED: &str = "Letter spaced";
const EMOJI: &str = "Hi 😀 there";
const TALL: &str = "Tall lines";
const STYLED: &str = "Styled runs";

fn style(align: TextAlign) -> TextStyle {
    TextStyle { text_align: Some(align), ..TextStyle::DEFAULT }
}

#[composable]
fn Cases() {
    let styled = AnnotatedString::build(|builder| {
        builder.append("Styled ");
        builder.with_style(SpanStyle { font_size: 20.sp(), ..SpanStyle::DEFAULT }, |builder| {
            builder.append("runs");
        });
    });
    Column!(modifier = Modifier.fill_max_size(), {
        BasicText!(SHORT);
        BasicText!(CENTERED, style = style(TextAlign::Center));
        BasicText!(
            CENTERED_WIDE,
            modifier = Modifier.fill_max_width(),
            style = style(TextAlign::Center)
        );
        BasicText!(END_WIDE, modifier = Modifier.fill_max_width(), style = style(TextAlign::End));
        BasicText!(WRAPPING);
        BasicText!(ELLIPSIZED, max_lines = 1, overflow = TextOverflow::Ellipsis);
        BasicText!(UNWRAPPED, soft_wrap = false);
        BasicText!(TWO_LINES);
        BasicText!(
            HALF_MIN,
            modifier = Modifier.layout(|scope, measurable, constraints| {
                let half = Constraints::new(
                    constraints.max_width / 2,
                    constraints.max_width,
                    0,
                    constraints.max_height,
                );
                let placeable = scope.measure(measurable, half);
                scope.place(&placeable, 0, 0);
                scope.layout(placeable.width, placeable.height)
            }),
            style = style(TextAlign::Center)
        );
        BasicText!(HEBREW);
        BasicText!(HEBREW_WIDE, modifier = Modifier.fill_max_width());
        BasicText!(ARABIC_END, style = style(TextAlign::End));
        BasicText!(JUSTIFIED, style = style(TextAlign::Justify));
        BasicText!(SPACED, style = TextStyle { letter_spacing: 2.sp(), ..TextStyle::DEFAULT });
        BasicText!(EMOJI);
        BasicText!(TALL, style = TextStyle { line_height: 30.sp(), ..TextStyle::DEFAULT });
        BasicAnnotatedText!(styled.clone());
    });
}


/// Turns compose-rs's `REUSE_PARAGRAPH_AT_SAME_LAYOUT_WIDTH` on or off on `host` (D551).
fn reuse_at_same_width(host: &mut compose::ui::platform::UiHost, on: bool) {
    let optimizations = host
        .optimizations()
        .with(compose::ui::platform::Optimization::REUSE_PARAGRAPH_AT_SAME_LAYOUT_WIDTH, on);
    host.set_optimizations(optimizations);
}

fn counted(width: i32) -> (ComposeTestRule, TextLayoutCounter) {
    let (measurer, counter) = CountingTextMeasurer::new(SkiaTextMeasurer::new());
    let host = UiHost::new(IntSize::new(width, 3000), Density::new(1.0), Box::new(measurer));
    (ComposeTestRule::with_host(host, Cases), counter)
}

fn resize(ui: &mut ComposeTestRule, width: i32, reuse: bool) {
    reuse_at_same_width(ui.host(), reuse);
    ui.host().resize(IntSize::new(width, 3000));
    ui.wait_for_idle();
    reuse_at_same_width(ui.host(), false);
}

#[test]
fn reused_paragraphs_draw_exactly_what_new_layouts_draw() {
    let widths = [400, 390, 420, 333, 300, 250, 200, 151, 150, 120, 100, 80, 60, 60, 40, 400, 395];
    let (mut parity, parity_counts) = counted(widths[0]);
    let (mut reusing, reusing_counts) = counted(widths[0]);
    parity_counts.clear();
    reusing_counts.clear();
    for width in &widths[1..] {
        resize(&mut parity, *width, false);
        resize(&mut reusing, *width, true);
        assert_eq!(
            draw_snapshot(reusing.host()),
            draw_snapshot(parity.host()),
            "the same frame at width {width}"
        );
    }
    let changes = widths.windows(2).filter(|pair| pair[0] != pair[1]).count();
    for text in [SHORT, CENTERED, WRAPPING, HEBREW, ARABIC_END, SPACED, EMOJI, TALL, STYLED] {
        assert_eq!(parity_counts.of(text).total(), changes, "{text:?}");
    }
    // Texts that fit keep their layout over most widths: laid out again only below their own
    // width and back.
    for text in [SHORT, CENTERED, HEBREW, ARABIC_END, SPACED, EMOJI, TALL, STYLED, TWO_LINES] {
        let reused = reusing_counts.of(text).total();
        assert!(reused <= changes / 2, "{text:?}: {reused} layouts of {changes}");
    }
    assert_eq!(reusing_counts.of(UNWRAPPED).total(), 0);
    // Laid out at the whole width, wrapped or ellipsized: as Compose's rule.
    for text in [CENTERED_WIDE, END_WIDE, HEBREW_WIDE, WRAPPING, ELLIPSIZED, JUSTIFIED] {
        assert_eq!(reusing_counts.of(text), parity_counts.of(text), "{text:?}");
    }
}

#[test]
fn a_width_below_the_texts_own_lays_it_out_again() {
    let (mut ui, counter) = counted(400);
    reuse_at_same_width(ui.host(), true);
    let own = {
        let snapshot = draw_snapshot(ui.host());
        let line =
            snapshot.lines().find(|line| line.contains("\"Short label\"")).unwrap().to_string();
        line.split("size: ").nth(1).unwrap().split('x').next().unwrap().parse::<i32>().unwrap()
    };
    counter.clear();
    resize(&mut ui, own, true);
    assert_eq!(counter.of(SHORT).total(), 0, "its own width holds it");
    resize(&mut ui, own - 1, true);
    assert_eq!(counter.of(SHORT).total(), 1, "one pixel less wraps it");
    reuse_at_same_width(ui.host(), false);
}
