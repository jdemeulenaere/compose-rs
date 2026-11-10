//! Colors of other spaces than sRGB, drawn by Skia: converted, not read as sRGB values.

use compose::ui::{
    Color, Offset, Paint, Rect, Shader, TileMode,
    colorspace::ColorSpaces,
    platform::{DisplayList, DrawOp, FixedTextMeasurer, Gradient, PaintShader, Renderer},
};
use compose_skia::RasterRenderer;

fn render(list: &DisplayList) -> RasterRenderer {
    let mut renderer = RasterRenderer::new(10, 10);
    renderer.render(list, Color::WHITE, 1.0, &FixedTextMeasurer::default());
    renderer
}

#[track_caller]
fn assert_near(expected: Color, actual: Color) {
    let (expected_argb, actual_argb) = (expected.to_argb(), actual.to_argb());
    for shift in [24, 16, 8, 0] {
        let (want, got) = ((expected_argb >> shift) as u8, (actual_argb >> shift) as u8);
        assert!(want.abs_diff(got) <= 1, "{actual:?}, expected {expected:?}");
    }
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/PaintTest.kt, testColorLongConfiguration
#[test]
fn a_display_p3_color_draws_as_its_srgb_conversion() {
    let p3 = Color::new_with(0.8, 0.3, 0.2, 1.0, &ColorSpaces::DISPLAY_P3);
    let mut list = DisplayList::default();
    list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 10.0, 10.0), Paint::fill(p3)));
    let pixel = render(&list).pixel(5, 5);
    assert_near(p3.convert(&ColorSpaces::SRGB), pixel);
    // Not its values read as sRGB's.
    assert_ne!(pixel, Color::new(0.8, 0.3, 0.2, 1.0));
    // An Adobe RGB orange is sRGB's orange.
    let adobe = Color::new_with(0.8916, 0.4980, 0.1168, 1.0, &ColorSpaces::ADOBE_RGB);
    let mut list = DisplayList::default();
    list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 10.0, 10.0), Paint::fill(adobe)));
    assert_near(Color::from_argb(0xFFFF_8000), render(&list).pixel(5, 5));
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/PaintTest.kt, testColorLongConfiguration
#[test]
fn a_gradient_of_another_space_draws_its_colors_converted() {
    let start = Color::new_with(0.2, 0.6, 0.3, 1.0, &ColorSpaces::DISPLAY_P3);
    let end = Color::new_with(0.7, 0.2, 0.5, 1.0, &ColorSpaces::DISPLAY_P3);
    let gradient =
        Gradient { colors: vec![start, end].into(), stops: None, tile_mode: TileMode::Clamp };
    let mut list = DisplayList::default();
    let shader = list.push_shader(PaintShader {
        shader: Shader::LinearGradient {
            from: Offset::new(2.0, 0.0),
            to: Offset::new(8.0, 0.0),
            gradient,
        },
        transform: None,
        alpha: 1.0,
    });
    list.push(DrawOp::DrawRect(
        Rect::new(0.0, 0.0, 10.0, 10.0),
        Paint::fill(Color::BLACK).with_shader(shader),
    ));
    let mut renderer = render(&list);
    assert_near(start.convert(&ColorSpaces::SRGB), renderer.pixel(0, 5));
    assert_near(end.convert(&ColorSpaces::SRGB), renderer.pixel(9, 5));
}
