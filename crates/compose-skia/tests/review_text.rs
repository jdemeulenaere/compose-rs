//! Adversarial review of styled text with SkParagraph: annotated strings' spans.

use compose::ui::{
    AnnotatedString, Color, Constraints, Density, SpanStyle, TextLayoutOptions, TextMeasurer,
    TextStyle,
};
use compose_skia::SkiaTextMeasurer;

/// A span whose byte range doesn't fall on character boundaries (as `add_style` takes byte
/// offsets, an off-by-one over a multi-byte character, e.g. "é" or an emoji) makes the Skia
/// measurer panic: `layout_runs` slices `&text[range]` with the run's raw byte range. Compose's
/// `AnnotatedString` indexes UTF-16 units and lays out a span ending inside a character without
/// crashing (the run boundaries snap to the characters). Expected: no panic, the text laid out.
#[test]
fn a_span_inside_a_multibyte_character_lays_out() {
    let text = AnnotatedString::build(|builder| {
        builder.append("héllo");
        // Byte 2 is inside "é" (bytes 1..3).
        builder.add_style(SpanStyle { color: Color::RED, ..SpanStyle::DEFAULT }, 0, 2);
    });
    let mut measurer = SkiaTextMeasurer::new();
    let laid_out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let paragraph = measurer.layout_annotated(
            &text,
            &TextStyle::DEFAULT,
            TextLayoutOptions::default(),
            Constraints::unbounded(),
            &Density::default(),
        );
        measurer.metrics(paragraph).line_count
    }));
    assert_eq!(laid_out.ok(), Some(1), "laying out a span ending inside \"é\" panicked");
}

/// Lock-in: spans past the end of the text are cut at its end.
#[test]
fn passes_a_span_past_the_end_is_cut_at_the_end() {
    let text = AnnotatedString::build(|builder| {
        builder.append("hello");
        builder.add_style(SpanStyle { color: Color::RED, ..SpanStyle::DEFAULT }, 2, 50);
        builder.add_style(SpanStyle { color: Color::BLUE, ..SpanStyle::DEFAULT }, 40, 60);
    });
    let mut measurer = SkiaTextMeasurer::new();
    let paragraph = measurer.layout_annotated(
        &text,
        &TextStyle::DEFAULT,
        TextLayoutOptions::default(),
        Constraints::unbounded(),
        &Density::default(),
    );
    assert_eq!(measurer.metrics(paragraph).line_count, 1);
    let empty = AnnotatedString::new("");
    let paragraph = measurer.layout_annotated(
        &empty,
        &TextStyle::DEFAULT,
        TextLayoutOptions::default(),
        Constraints::unbounded(),
        &Density::default(),
    );
    assert_eq!(measurer.metrics(paragraph).line_count, 1);
}
