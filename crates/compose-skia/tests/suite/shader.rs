//! Shaders, wide-gamut `F16` gradient rendering (`ShaderTest`), `BrushPainter`, and `ImageShader`
//! / `CompositeShader` pixel checks on Skia.

use std::sync::Arc;

use compose::foundation::layout::Box;
use compose::test::{ComposeTestRule, set_capture_renderer};
use compose::ui::canvas::{Canvas, CanvasDrawScope};
use compose::ui::colorspace::{ColorSpace, ColorSpaces};
use compose::ui::{
    Alignment, BlendMode, Brush, BrushPainter, Color, ColorFilter, ContentScale, CustomPainter,
    Density, ImageBitmap, ImageBitmapConfig, IntSize, LayoutDirection, Matrix, Modifier, Offset,
    Paint, PaintStyle, Painter, Rect, Shader, ShaderBrush, Size, TileMode, UnitExt,
};

fn install() {
    compose::ui::platform::set_bitmap_rasterizer(Arc::new(compose_skia::SkiaBitmapRasterizer));
}

fn draw_into_bitmap(image: &ImageBitmap, draw: impl FnOnce(&mut compose::ui::DrawScope<'_>)) {
    install();
    let mut canvas = Canvas::new(image);
    CanvasDrawScope::draw(
        &mut canvas,
        Density::new(1.0),
        LayoutDirection::Ltr,
        Size::new(image.width() as f32, image.height() as f32),
        draw,
    );
}

fn fill_white_bitmap(width: u32, height: u32) -> ImageBitmap {
    let image = ImageBitmap::new(width, height);
    let mut canvas = Canvas::new(&image);
    canvas.draw_rect(Rect::new(0.0, 0.0, width as f32, height as f32), Paint::fill(Color::WHITE));
    image
}

fn draw_brush_painter(
    painter: &BrushPainter,
    image: &ImageBitmap,
    size: Size,
    alpha: f32,
    color_filter: Option<&ColorFilter>,
) {
    install();
    let mut canvas = Canvas::new(image);
    let wrapped = Painter::new(BrushPainter::new(painter.brush.clone()));
    CanvasDrawScope::draw(&mut canvas, Density::new(1.0), LayoutDirection::Ltr, size, |scope| {
        wrapped.draw(scope, size, alpha, color_filter);
    });
}

fn verify_wide_color(expected: Color, actual: Color, space: &ColorSpace) {
    assert_eq!(actual.color_space(), space);
    assert!((expected.red() - actual.red()).abs() <= 0.01, "{expected:?} vs {actual:?}");
    assert!((expected.green() - actual.green()).abs() <= 0.01, "{expected:?} vs {actual:?}");
    assert!((expected.blue() - actual.blue()).abs() <= 0.01, "{expected:?} vs {actual:?}");
}

// ---- `ShaderTest` pixel tests -----------------------------------------------------------------

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/ShaderTest.kt, testLinearGradient
#[test]
fn linear_gradient_pixels() {
    let image = ImageBitmap::new(100, 100);
    let brush = Brush::linear_gradient_with(
        [(0.0, Color::RED), (0.5, Color::RED), (0.5, Color::BLUE), (1.0, Color::BLUE)],
        Offset::ZERO,
        Offset::new(0.0, 100.0),
        TileMode::Clamp,
    );
    draw_into_bitmap(&image, |scope| {
        let paint = scope.brush_paint(&brush, 1.0, PaintStyle::Fill);
        scope.draw_rect(scope.bounds(), paint);
    });
    let map = image.to_pixel_map();
    let (cx, cy) = (50, 50);
    assert_eq!(map.get(cx, cy - 5), Color::RED);
    assert_eq!(map.get(cx, cy + 5), Color::BLUE);
    assert_eq!(map.get(5, cy - 5), Color::RED);
    assert_eq!(map.get(5, cy + 5), Color::BLUE);
    assert_eq!(map.get(95, cy - 5), Color::RED);
    assert_eq!(map.get(95, cy + 5), Color::BLUE);
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/ShaderTest.kt, testLinearGradientColorLong
#[test]
fn linear_gradient_color_long_in_f16() {
    let red_p3 = Color::new_with(1.0, 0.0, 0.0, 1.0, &ColorSpaces::DISPLAY_P3);
    let blue_p3 = Color::new_with(0.0, 0.0, 1.0, 1.0, &ColorSpaces::DISPLAY_P3);
    let brush = Brush::linear_gradient_with(
        [(0.0, red_p3), (0.5, red_p3), (0.5, blue_p3), (1.0, blue_p3)],
        Offset::ZERO,
        Offset::new(0.0, 100.0),
        TileMode::Clamp,
    );
    for space in [&*ColorSpaces::EXTENDED_SRGB, &*ColorSpaces::SRGB, &*ColorSpaces::DISPLAY_P3] {
        let image = ImageBitmap::new_with(100, 100, ImageBitmapConfig::F16, true, space);
        draw_into_bitmap(&image, |scope| {
            let paint = scope.brush_paint(&brush, 1.0, PaintStyle::Fill);
            scope.draw_rect(scope.bounds(), paint);
        });
        let expected_red = red_p3.convert(space);
        let expected_blue = blue_p3.convert(space);
        for x in [5, 50, 95] {
            verify_wide_color(expected_red, image.color(x, 45), space);
            verify_wide_color(expected_blue, image.color(x, 55), space);
        }
    }
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/ShaderTest.kt, testRadialGradient
#[test]
fn radial_gradient_pixels() {
    let image = ImageBitmap::new(100, 100);
    let brush = Brush::radial_gradient_with(
        [(0.0, Color::RED), (0.5, Color::RED), (0.5, Color::BLUE), (1.0, Color::BLUE)],
        Offset::new(50.0, 50.0),
        50.0,
        TileMode::Clamp,
    );
    draw_into_bitmap(&image, |scope| {
        let paint = scope.brush_paint(&brush, 1.0, PaintStyle::Fill);
        scope.draw_circle(Offset::new(50.0, 50.0), 50.0, paint);
    });
    let map = image.to_pixel_map();
    for (x, y) in [(50, 50), (50, 30), (70, 50), (50, 70), (30, 50)] {
        assert_eq!(map.get(x, y), Color::RED);
    }
    for (x, y) in [(50, 20), (80, 50), (50, 80), (20, 50)] {
        assert_eq!(map.get(x, y), Color::BLUE);
    }
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/ShaderTest.kt, testRadialGradientColorLong
#[test]
fn radial_gradient_color_long_in_f16() {
    let red_p3 = Color::new_with(1.0, 0.0, 0.0, 1.0, &ColorSpaces::DISPLAY_P3);
    let blue_p3 = Color::new_with(0.0, 0.0, 1.0, 1.0, &ColorSpaces::DISPLAY_P3);
    let brush = Brush::radial_gradient_with(
        [(0.0, red_p3), (0.5, red_p3), (0.5, blue_p3), (1.0, blue_p3)],
        Offset::new(50.0, 50.0),
        50.0,
        TileMode::Clamp,
    );
    let space = &*ColorSpaces::EXTENDED_SRGB;
    let image = ImageBitmap::new_with(100, 100, ImageBitmapConfig::F16, true, space);
    draw_into_bitmap(&image, |scope| {
        let paint = scope.brush_paint(&brush, 1.0, PaintStyle::Fill);
        scope.draw_circle(Offset::new(50.0, 50.0), 50.0, paint);
    });
    let expected_red = red_p3.convert(space);
    let expected_blue = blue_p3.convert(space);
    for (x, y) in [(50, 50), (50, 30), (70, 50), (50, 70), (30, 50)] {
        verify_wide_color(expected_red, image.color(x, y), space);
    }
    for (x, y) in [(50, 20), (80, 50), (50, 80), (20, 50)] {
        verify_wide_color(expected_blue, image.color(x, y), space);
    }
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/ShaderTest.kt, testSweepGradient
#[test]
fn sweep_gradient_pixels() {
    let image = ImageBitmap::new(100, 100);
    let brush = Brush::sweep_gradient_with(
        [(0.0, Color::RED), (0.5, Color::RED), (0.5, Color::BLUE), (1.0, Color::BLUE)],
        Offset::new(50.0, 50.0),
    );
    draw_into_bitmap(&image, |scope| {
        let paint = scope.brush_paint(&brush, 1.0, PaintStyle::Fill);
        scope.draw_rect(scope.bounds(), paint);
    });
    let map = image.to_pixel_map();
    for x in [5, 50, 95] {
        assert_eq!(map.get(x, 55), Color::RED);
        assert_eq!(map.get(x, 45), Color::BLUE);
    }
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/ShaderTest.kt, testSweepGradientColorLong
#[test]
fn sweep_gradient_color_long_in_f16() {
    let red_p3 = Color::new_with(1.0, 0.0, 0.0, 1.0, &ColorSpaces::DISPLAY_P3);
    let blue_p3 = Color::new_with(0.0, 0.0, 1.0, 1.0, &ColorSpaces::DISPLAY_P3);
    let brush = Brush::sweep_gradient_with(
        [(0.0, red_p3), (0.5, red_p3), (0.5, blue_p3), (1.0, blue_p3)],
        Offset::new(50.0, 50.0),
    );
    let space = &*ColorSpaces::EXTENDED_SRGB;
    let image = ImageBitmap::new_with(100, 100, ImageBitmapConfig::F16, true, space);
    draw_into_bitmap(&image, |scope| {
        let paint = scope.brush_paint(&brush, 1.0, PaintStyle::Fill);
        scope.draw_rect(scope.bounds(), paint);
    });
    let expected_red = red_p3.convert(space);
    let expected_blue = blue_p3.convert(space);
    for x in [5, 50, 95] {
        verify_wide_color(expected_red, image.color(x, 55), space);
        verify_wide_color(expected_blue, image.color(x, 45), space);
    }
}

// ---- `BrushPainterTest` (8 tests) -------------------------------------------------------------

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/painter/BrushPainterTest.kt, testBrushPainter
#[test]
fn brush_painter_draws_vertical_gradient() {
    let painter = BrushPainter::new(Brush::vertical_gradient([
        (0.0, Color::RED),
        (0.5, Color::RED),
        (0.5, Color::BLUE),
        (1.0, Color::BLUE),
    ]));
    let image = fill_white_bitmap(100, 100);
    draw_brush_painter(&painter, &image, Size::new(100.0, 100.0), 1.0, None);
    let map = image.to_pixel_map();
    for &(x, y) in &[(0, 0), (99, 0), (0, 49), (99, 49)] {
        assert_eq!(map.get(x, y), Color::RED);
    }
    for &(x, y) in &[(0, 50), (99, 50), (0, 99), (99, 99)] {
        assert_eq!(map.get(x, y), Color::BLUE);
    }
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/painter/BrushPainterTest.kt, testCompositeBrush
#[test]
fn brush_painter_composite_brush() {
    let bitmap = ImageBitmap::new(100, 100);
    draw_into_bitmap(&bitmap, |scope| {
        let stroke = Paint::stroke(Color::BLACK, 20.0);
        scope.draw_rect(scope.bounds(), stroke);
    });
    let bitmap_brush: Brush =
        ShaderBrush::new(Shader::image(&bitmap, TileMode::Clamp, TileMode::Clamp)).into();
    let gradient_brush = Brush::vertical_gradient([
        (0.0, Color::RED),
        (0.5, Color::RED),
        (0.5, Color::BLUE),
        (1.0, Color::BLUE),
    ]);
    let dst = fill_white_bitmap(100, 100);
    let painter =
        BrushPainter::new(Brush::composite(bitmap_brush, gradient_brush, BlendMode::SrcIn));
    draw_brush_painter(&painter, &dst, Size::new(100.0, 100.0), 1.0, None);
    let map = dst.to_pixel_map();
    for &(x, y) in &[(0, 0), (99, 0), (0, 49), (99, 49)] {
        assert_eq!(map.get(x, y), Color::RED);
    }
    for &(x, y) in &[(0, 50), (99, 50), (0, 99), (99, 99)] {
        assert_eq!(map.get(x, y), Color::BLUE);
    }
    assert_ne!(map.get(50, 49), Color::RED);
    assert_ne!(map.get(59, 49), Color::BLUE);
    assert_ne!(map.get(50, 51), Color::RED);
    assert_ne!(map.get(50, 51), Color::BLUE);
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/painter/BrushPainterTest.kt, testBrushPainterAlphaApplied
#[test]
fn brush_painter_applies_alpha() {
    let painter = BrushPainter::new(Brush::vertical_gradient([
        (0.0, Color::RED),
        (0.5, Color::RED),
        (0.5, Color::BLUE),
        (1.0, Color::BLUE),
    ]));
    let image = fill_white_bitmap(100, 100);
    draw_brush_painter(&painter, &image, Size::new(100.0, 100.0), 0.5, None);
    let expected_red = Color::RED.with_alpha(0.5).composite_over(Color::WHITE);
    let expected_blue = Color::BLUE.with_alpha(0.5).composite_over(Color::WHITE);
    let map = image.to_pixel_map();
    let near = |got: Color, want: Color| {
        ((got.to_argb() >> 16) as u8).abs_diff((want.to_argb() >> 16) as u8) <= 2
            && ((got.to_argb() >> 8) as u8).abs_diff((want.to_argb() >> 8) as u8) <= 2
            && (got.to_argb() as u8).abs_diff(want.to_argb() as u8) <= 2
    };
    for &(x, y) in &[(0, 0), (99, 0), (0, 49), (99, 49)] {
        assert!(near(map.get(x, y), expected_red), "{:?} vs {:?}", map.get(x, y), expected_red);
    }
    for &(x, y) in &[(0, 50), (99, 50), (0, 99), (99, 99)] {
        assert!(near(map.get(x, y), expected_blue), "{:?} vs {:?}", map.get(x, y), expected_blue);
    }
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/painter/BrushPainterTest.kt, testBrushPainterTint
#[test]
fn brush_painter_applies_tint() {
    let painter = BrushPainter::new(Brush::vertical_gradient([
        (0.0, Color::RED),
        (0.5, Color::RED),
        (0.5, Color::BLUE),
        (1.0, Color::BLUE),
    ]));
    let image = fill_white_bitmap(100, 100);
    let tint = ColorFilter::tint(Color::CYAN);
    draw_brush_painter(&painter, &image, Size::new(100.0, 100.0), 1.0, Some(&tint));
    let map = image.to_pixel_map();
    for &(x, y) in &[(0, 0), (99, 0), (0, 49), (99, 49), (0, 50), (99, 50), (0, 99), (99, 99)] {
        assert_eq!(map.get(x, y), Color::CYAN);
    }
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/painter/BrushPainterTest.kt, testBrushPainterEquals, testBrushPainterHashCode, testBrushPainterToString, testBrushPainterIntrinsicSize
#[test]
fn brush_painter_equality_debug_and_intrinsic_size() {
    let brush1 =
        Brush::vertical_gradient([(0.0, Color::RED), (0.3, Color::BLUE), (0.7, Color::GREEN)]);
    let brush2 =
        Brush::vertical_gradient([(0.0, Color::RED), (0.3, Color::BLUE), (0.7, Color::GREEN)]);
    assert_eq!(
        Painter::new(BrushPainter::new(brush1.clone())),
        Painter::new(BrushPainter::new(brush2))
    );
    let vertical =
        Brush::vertical_gradient_with([Color::WHITE, Color::BLACK], 0.0, 100.0, TileMode::Clamp);
    let painter = BrushPainter::new(vertical.clone());
    assert_eq!(CustomPainter::intrinsic_size(&painter), Size::new(0.0, 100.0));
    assert!(format!("{:?}", painter).contains("BrushPainter"));
}

#[test]
fn image_shader_tile_modes_and_transform_render() {
    let tile = ImageBitmap::new(2, 1);
    {
        let mut canvas = Canvas::new(&tile);
        canvas.draw_rect(Rect::new(0.0, 0.0, 1.0, 1.0), Paint::fill(Color::RED));
        canvas.draw_rect(Rect::new(1.0, 0.0, 2.0, 1.0), Paint::fill(Color::BLUE));
    }
    let draw = |tx: TileMode, ty: TileMode, transform: Option<Matrix>| {
        let shader = ShaderBrush::new(Shader::image(&tile, tx, ty));
        shader.set_transform(transform);
        let brush = Brush::from(shader);
        let dst = fill_white_bitmap(10, 4);
        draw_into_bitmap(&dst, |scope| {
            let paint = scope.brush_paint(&brush, 1.0, PaintStyle::Fill);
            scope.draw_rect(scope.bounds(), paint);
        });
        dst.to_pixel_map()
    };
    let repeated = draw(TileMode::Repeated, TileMode::Clamp, None);
    assert_eq!(repeated.get(0, 0), Color::RED);
    assert_eq!(repeated.get(1, 0), Color::BLUE);
    assert_eq!(repeated.get(2, 0), Color::RED);
    assert_eq!(repeated.get(3, 0), Color::BLUE);

    let mirror = draw(TileMode::Mirror, TileMode::Clamp, None);
    assert_eq!(mirror.get(0, 0), Color::RED);
    assert_eq!(mirror.get(1, 0), Color::BLUE);
    assert_eq!(mirror.get(2, 0), Color::BLUE);
    assert_eq!(mirror.get(3, 0), Color::RED);

    let decal = draw(TileMode::Decal, TileMode::Decal, None);
    assert_eq!(decal.get(0, 0), Color::RED);
    assert_eq!(decal.get(1, 0), Color::BLUE);
    assert_eq!(decal.get(5, 0), Color::WHITE);

    let shifted = draw(TileMode::Decal, TileMode::Decal, Some(Matrix::translation(4.0, 0.0)));
    assert_eq!(shifted.get(0, 0), Color::WHITE);
    assert_eq!(shifted.get(4, 0), Color::RED);
    assert_eq!(shifted.get(5, 0), Color::BLUE);
}

#[test]
fn modulate_alpha_layer_fades_shader_brush() {
    set_capture_renderer(compose_skia::raster_colors);
    let mut ui = ComposeTestRule::sized(IntSize::new(20, 20), move || {
        let brush = Brush::from(ShaderBrush::new(Shader::linear_gradient(
            Offset::ZERO,
            Offset::new(20.0, 0.0),
            [Color::RED, Color::RED],
            TileMode::Clamp,
        )));
        Box!(
            modifier = Modifier
                .size(20.dp())
                .background(Color::WHITE)
                .graphics_layer(|scope| {
                    scope.compositing_strategy = compose::ui::CompositingStrategy::ModulateAlpha;
                    scope.alpha = 0.5;
                })
                .test_tag("TAG"),
            {
                let brush = brush.clone();
                Box!(
                    modifier = Modifier.size(20.dp()).paint_with(
                        Painter::new(BrushPainter::new(brush.clone())),
                        false,
                        Alignment::CENTER,
                        ContentScale::FillBounds,
                        1.0,
                        None,
                    )
                );
            }
        );
    });
    let pixel = ui.on_node_with_tag("TAG").capture_to_image().pixel(10, 10);
    let expected = Color::RED.with_alpha(0.5).composite_over(Color::WHITE);
    assert!(
        ((pixel.to_argb() >> 8) as u8).abs_diff((expected.to_argb() >> 8) as u8) <= 3,
        "{pixel:?} vs {expected:?}"
    );
}
