//! Compose's `TextTest.kt`, adapted: `Text` takes the style `ProvideTextStyle` gives, its own
//! `style` instead, and its parameters over both. Compose reads the style a text was laid out
//! with from `onTextLayout`'s `layoutInput`; here a measurer records it (a private helper: the
//! test harness has no such probe). Letter spacing is in sp (compose-rs has no `em` yet).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use compose_material3::*;
use compose_test::*;
use compose_ui::platform::{
    FixedTextMeasurer, ParagraphHandle, ParagraphMetrics, TextLayoutOptions, TextMeasurer,
    TextPosition,
};
use compose_ui::{
    AnnotatedString, Color, Constraints, Density, FontStyle, IntSize, Offset, Rect, TextAlign,
    TextStyle, UnitExt,
};

/// The style each text was last laid out (or restyled) with.
type Styles = Rc<RefCell<HashMap<String, TextStyle>>>;

/// The fixed-advance measurer, recording the styles it's given.
struct Recording {
    inner: FixedTextMeasurer,
    styles: Styles,
}

impl Recording {
    fn record(&self, text: &str, style: &TextStyle) {
        self.styles.borrow_mut().insert(text.to_string(), style.clone());
    }
}

impl TextMeasurer for Recording {
    fn layout(
        &mut self,
        text: &str,
        style: &TextStyle,
        options: TextLayoutOptions,
        constraints: Constraints,
        density: &Density,
    ) -> ParagraphHandle {
        self.record(text, style);
        self.inner.layout(text, style, options, constraints, density)
    }

    fn layout_annotated(
        &mut self,
        text: &AnnotatedString,
        style: &TextStyle,
        options: TextLayoutOptions,
        constraints: Constraints,
        density: &Density,
    ) -> ParagraphHandle {
        self.record(text.as_str(), style);
        self.inner.layout_annotated(text, style, options, constraints, density)
    }

    fn layout_probe(
        &mut self,
        measured: Option<ParagraphHandle>,
        text: &AnnotatedString,
        style: &TextStyle,
        options: TextLayoutOptions,
        constraints: Constraints,
        density: &Density,
    ) -> ParagraphHandle {
        self.record(text.as_str(), style);
        self.inner.layout_probe(measured, text, style, options, constraints, density)
    }

    fn metrics(&self, paragraph: ParagraphHandle) -> ParagraphMetrics {
        self.inner.metrics(paragraph)
    }

    fn did_exceed_max_lines(&self, paragraph: ParagraphHandle) -> bool {
        self.inner.did_exceed_max_lines(paragraph)
    }

    fn max_intrinsic_width(&self, paragraph: ParagraphHandle) -> Option<f32> {
        self.inner.max_intrinsic_width(paragraph)
    }

    fn relayout(
        &mut self,
        paragraph: ParagraphHandle,
        constraints: Constraints,
    ) -> Option<ParagraphHandle> {
        self.inner.relayout(paragraph, constraints)
    }

    fn restyle(
        &mut self,
        paragraph: ParagraphHandle,
        style: &TextStyle,
    ) -> Option<ParagraphHandle> {
        let text = self.inner.text(paragraph).to_string();
        let restyled = self.inner.restyle(paragraph, style)?;
        self.record(&text, style);
        Some(restyled)
    }

    fn hit_test(&self, paragraph: ParagraphHandle, position: Offset) -> TextPosition {
        self.inner.hit_test(paragraph, position)
    }

    fn is_right_to_left(&self, paragraph: ParagraphHandle) -> bool {
        self.inner.is_right_to_left(paragraph)
    }

    fn cursor_rect(&self, paragraph: ParagraphHandle, offset: usize) -> Rect {
        self.inner.cursor_rect(paragraph, offset)
    }

    fn selection_rects(
        &self,
        paragraph: ParagraphHandle,
        range: std::ops::Range<usize>,
    ) -> Vec<Rect> {
        self.inner.selection_rects(paragraph, range)
    }

    fn text(&self, paragraph: ParagraphHandle) -> &str {
        self.inner.text(paragraph)
    }

    fn release(&mut self, paragraph: ParagraphHandle) {
        self.inner.release(paragraph)
    }
}

/// The style "TestText" was laid out with, in `content`.
fn laid_out_style(content: impl Fn() + 'static) -> TextStyle {
    let styles = Styles::default();
    let measurer = Recording { inner: FixedTextMeasurer::new(), styles: styles.clone() };
    let _ui =
        compose_test_with(IntSize::new(400, 400), Density::default(), Box::new(measurer), content);
    styles.borrow().get(TEST_TEXT).cloned().expect("laid out")
}

const TEST_TEXT: &str = "TestText";

/// As the test's `ExpectedTextStyle`.
fn expected_style() -> TextStyle {
    TextStyle {
        color: Color::BLUE,
        text_align: Some(TextAlign::End),
        font_size: 32.sp(),
        font_style: Some(FontStyle::Italic),
        letter_spacing: 0.3.sp(),
        ..TextStyle::DEFAULT
    }
}

/// The test's other style.
fn test_style() -> TextStyle {
    TextStyle {
        color: Color::GREEN,
        text_align: Some(TextAlign::Center),
        font_size: 16.sp(),
        font_style: Some(FontStyle::Normal),
        letter_spacing: 0.6.sp(),
        ..TextStyle::DEFAULT
    }
}

/// The properties the tests compare.
fn compared(style: TextStyle) -> (Color, Option<TextAlign>, f32, Option<FontStyle>, f32) {
    (
        style.color,
        style.text_align,
        style.font_size.value(),
        style.font_style,
        style.letter_spacing.value(),
    )
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextTest.kt, inheritsThemeTextStyle
#[test]
fn a_text_takes_the_provided_style() {
    let style = laid_out_style(|| {
        ProvideTextStyle!(expected_style(), {
            Text!("{}", TEST_TEXT);
        });
    });
    assert_eq!(compared(style), compared(expected_style()));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextTest.kt, settingCustomTextStyle
#[test]
fn a_text_given_a_style_takes_it_instead() {
    let style = laid_out_style(|| {
        ProvideTextStyle!(expected_style(), {
            Text!("{}", TEST_TEXT, style = test_style());
        });
    });
    assert_eq!(compared(style), compared(test_style()));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextTest.kt, settingParametersExplicitly
#[test]
fn a_texts_parameters_override_the_provided_style() {
    let style = laid_out_style(|| {
        ProvideTextStyle!(expected_style(), {
            Text!(
                "{}",
                TEST_TEXT,
                color = Color::GREEN,
                text_align = Some(TextAlign::Center),
                font_size = 16.sp(),
                font_style = Some(FontStyle::Normal),
                letter_spacing = 0.6.sp(),
            );
        });
    });
    assert_eq!(compared(style), compared(test_style()));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/TextTest.kt, settingColorAndTextStyle
#[test]
fn a_texts_parameters_override_its_own_style() {
    let style = laid_out_style(|| {
        ProvideTextStyle!(expected_style(), {
            Text!(
                "{}",
                TEST_TEXT,
                color = Color::GREEN,
                text_align = Some(TextAlign::Center),
                font_size = 16.sp(),
                font_style = Some(FontStyle::Normal),
                letter_spacing = 0.6.sp(),
                style = expected_style(),
            );
        });
    });
    assert_eq!(compared(style), compared(test_style()));
}

/// `ProvideTextStyle` merges into what's there: what it doesn't set stays (Compose's
/// `ProvideTextStyle`, `LocalTextStyle.current.merge(value)`).
#[test]
fn provide_text_style_merges_into_the_current_style() {
    let style = laid_out_style(|| {
        MaterialTheme!({
            ProvideTextStyle!(TextStyle { color: Color::BLUE, ..TextStyle::DEFAULT }, {
                Text!("{}", TEST_TEXT);
            });
        });
    });
    // The theme's body large, blue.
    let body = Typography::default().body_large;
    assert_eq!(style.color, Color::BLUE);
    assert_eq!((style.font_size, style.line_height), (body.font_size, body.line_height));
}
