//! Line heights with real text (SkParagraph, the system's fonts), as Compose spreads them
//! (D571): a style without a line height style takes `LineHeightStyle.Default` (proportional,
//! both ends trimmed: Compose's `createCharSequence` uses its plain `LineHeightSpan` only with
//! `includeFontPadding`, which a style without a platform style doesn't have), Material 3's
//! style centers the height and trims nothing. Each case states Compose's heights.

use compose::ui::{
    Constraints, Density, LineHeightAlignment, LineHeightStyle, LineHeightTrim, TextStyle,
    TextUnit,
    platform::{TextLayoutOptions, TextMeasurer},
};
use compose_skia::SkiaTextMeasurer;

fn height(text: &str, style: &TextStyle) -> i32 {
    let mut measurer = SkiaTextMeasurer::new();
    let paragraph = measurer.layout(
        text,
        style,
        TextLayoutOptions::default(),
        Constraints::new(0, 10_000, 0, i32::MAX),
        &Density::default(),
    );
    measurer.metrics(paragraph).size.height
}

const TALL: TextStyle = TextStyle {
    font_size: TextUnit::sp(16.0),
    line_height: TextUnit::sp(40.0),
    ..TextStyle::DEFAULT
};

/// No line height style: a single line keeps its font's height (trimmed at both ends: 19 px
/// with Roboto on Android), and two lines are the font's height and one line height (59 px).
#[test]
fn a_style_without_a_line_height_style_trims_both_ends() {
    let font = height("Hello", &TextStyle { font_size: TextUnit::sp(16.0), ..TextStyle::DEFAULT });
    assert!(font < 30, "the font's own height: {font}");
    let one = height("Hello", &TALL);
    assert_eq!(one, font, "one line, trimmed at both ends");
    let two = height("Hello\nWorld", &TALL);
    assert!((two - (font + 40)).abs() <= 1, "two lines: {two}, the font's {font} and 40");
    // The same as `LineHeightStyle::DEFAULT` said outright.
    let default = TextStyle { line_height_style: Some(LineHeightStyle::DEFAULT), ..TALL };
    assert_eq!((height("Hello", &default), height("Hello\nWorld", &default)), (one, two));
}

/// Material 3's style (centered, untrimmed): every line the line height.
#[test]
fn a_centered_untrimmed_style_makes_every_line_the_line_height() {
    let material = TextStyle {
        line_height_style: Some(LineHeightStyle::new(
            LineHeightAlignment::CENTER,
            LineHeightTrim::None,
        )),
        ..TALL
    };
    assert_eq!(height("Hello", &material), 40);
    assert_eq!(height("Hello\nWorld", &material), 80);
    // An empty text's line and an empty last line are line heights too, as Compose's
    // `TextLayout` makes them (`getLastLineMetrics`: a zero-width space with the span).
    assert_eq!(height("", &material), 40);
    assert_eq!(height("Hello\n", &material), 80);
}
