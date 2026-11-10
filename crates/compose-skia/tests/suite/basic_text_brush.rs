//! Compose's `BasicTextBrushTest` (foundation, device tests): a `BasicText` whose style's brush
//! is toggled (solid, gradient, or custom `ShaderBrush`) draws in the new brush, drawn by Skia
//! in the sample font (as Compose's test font) and checked for the colors its pixels hold.

use compose::foundation::*;
use compose::runtime::*;
use compose::test::*;
use compose::ui::{Brush, Color, Density, IntSize, Modifier, UnitExt};
use compose_skia::SkiaTextMeasurer;

const TAG: &str = "TAG";

/// "Hello" at 20 px in the sample font, painted with `brush()` (read as it's composed) over
/// black, at density 1.
fn hello(brush: impl Fn() -> Brush + 'static) -> ComposeTestRule {
    set_capture_renderer(compose_skia::raster_colors);
    let mut ui = compose_test_with(
        IntSize::new(400, 200),
        Density::new(1.0),
        std::boxed::Box::new(SkiaTextMeasurer::new()),
        move || {
            BasicText!(
                "Hello",
                style = TextStyle {
                    brush: Some(brush()),
                    font_family: Some(basic_measure_font().to_font_family()),
                    font_size: 20.sp(),
                    ..TextStyle::DEFAULT
                },
                modifier = Modifier.background(Color::BLACK).test_tag(TAG),
            );
        },
    );
    ui.wait_for_idle();
    ui
}

/// Black and `shown` in the text's pixels, not `hidden`.
fn assert_colors(ui: &mut ComposeTestRule, shown: Color, hidden: Color) {
    let image = ui.on_node_with_tag(TAG).capture_to_image();
    image.assert_contains_color(Color::BLACK);
    image.assert_contains_color(shown);
    image.assert_does_not_contain_color(hidden);
}

/// The brush `first`, then `second`: red, then blue.
fn toggled(first: Brush, second: Brush) {
    let brush = mutable_state_of(first);
    let current = brush.handle();
    let mut ui = hello(move || current.get());
    assert_colors(&mut ui, Color::RED, Color::BLUE);
    brush.set(second);
    ui.wait_for_idle();
    assert_colors(&mut ui, Color::BLUE, Color::RED);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/BasicTextBrushTest.kt, toggleSolidColorBrush
#[test]
fn toggle_solid_color_brush() {
    toggled(Brush::Solid(Color::RED), Brush::Solid(Color::BLUE));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/BasicTextBrushTest.kt, togglePredefinedShaderBrush
#[test]
fn toggle_predefined_shader_brush() {
    toggled(
        Brush::horizontal_gradient([Color::RED, Color::RED]),
        Brush::horizontal_gradient([Color::BLUE, Color::BLUE]),
    );
}

#[derive(Clone)]
struct ToggleBrush(compose::runtime::State<bool>);

impl compose::ui::CustomShaderBrush for ToggleBrush {
    fn create_shader(&self, size: compose::ui::Size) -> compose::ui::Shader {
        let color = if self.0.get() { Color::RED } else { Color::BLUE };
        compose::ui::Shader::linear_gradient(
            compose::ui::Offset::ZERO,
            compose::ui::Offset::new(size.width.max(1.0), 0.0),
            [color, color],
            compose::ui::TileMode::Clamp,
        )
    }

    fn same(&self, other: &dyn compose::ui::CustomShaderBrush) -> bool {
        other.downcast_ref::<ToggleBrush>().is_some()
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/text/BasicTextBrushTest.kt, toggleCustomShaderBrush
#[test]
fn toggle_custom_shader_brush() {
    use compose::ui::{AnnotatedRange, AnnotatedString, Annotation, ShaderBrush, SpanStyle};
    use std::cell::Cell;
    use std::rc::Rc;

    set_capture_renderer(compose_skia::raster_colors);
    let is_red = mutable_state_of(true);
    let handle = is_red.as_state();
    let brush: Brush = ShaderBrush::custom(ToggleBrush(handle)).into();
    let recompositions = Rc::new(Cell::new(0usize));
    let recompositions_count = recompositions.clone();
    let mut ui = compose_test_with(
        IntSize::new(400, 200),
        Density::new(1.0),
        std::boxed::Box::new(SkiaTextMeasurer::new()),
        move || {
            recompositions_count.set(recompositions_count.get() + 1);
            BasicText!(
                "Hello",
                style = TextStyle {
                    brush: Some(brush.clone()),
                    font_family: Some(basic_measure_font().to_font_family()),
                    font_size: 20.sp(),
                    ..TextStyle::DEFAULT
                },
                modifier = Modifier.background(Color::BLACK).test_tag(TAG),
            );
        },
    );
    ui.wait_for_idle();
    assert_colors(&mut ui, Color::RED, Color::BLUE);
    assert_eq!(recompositions.get(), 1);
    is_red.set(false);
    ui.wait_for_idle();
    assert_colors(&mut ui, Color::BLUE, Color::RED);
    // State is read during draw (`Observation::Draw`), without recomposing.
    assert_eq!(recompositions.get(), 1);

    // Also works when custom ShaderBrush is placed inside a SpanStyle on AnnotatedString.
    let span_red = mutable_state_of(true);
    let span_brush: Brush = ShaderBrush::custom(ToggleBrush(span_red.as_state())).into();
    let text = AnnotatedString::with_annotations(
        "Hello",
        vec![AnnotatedRange::new(
            Annotation::SpanStyle(SpanStyle::DEFAULT.with_brush(Some(span_brush), 1.0)),
            0,
            5,
        )],
    );
    let mut span_ui = compose_test_with(
        IntSize::new(400, 200),
        Density::new(1.0),
        std::boxed::Box::new(SkiaTextMeasurer::new()),
        move || {
            BasicAnnotatedText!(
                text.clone(),
                style = TextStyle {
                    font_family: Some(basic_measure_font().to_font_family()),
                    font_size: 20.sp(),
                    ..TextStyle::DEFAULT
                },
                modifier = Modifier.background(Color::BLACK).test_tag(TAG),
            );
        },
    );
    span_ui.wait_for_idle();
    assert_colors(&mut span_ui, Color::RED, Color::BLUE);
    span_red.set(false);
    span_ui.wait_for_idle();
    assert_colors(&mut span_ui, Color::BLUE, Color::RED);
}
