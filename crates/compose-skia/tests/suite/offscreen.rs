//! Bitmaps drawn into: `ImageBitmap::new`, `Canvas`, `CanvasDrawScope`, `PixelMap`, and a
//! graphics layer's bitmap, rasterized by Skia.

use std::sync::Arc;

use compose::ui::{
    BlendMode, Color, ColorFilter, ColorMatrix, Density, GraphicsLayer, ImageBitmap,
    ImageBitmapConfig, IntSize, LayoutDirection, Offset, Paint, Rect, Size,
    canvas::{Canvas, CanvasDrawScope},
    colorspace::ColorSpaces,
};

fn install() {
    compose::ui::platform::set_bitmap_rasterizer(Arc::new(compose_skia::SkiaBitmapRasterizer));
}

/// A 100 × 100 bitmap of four quadrants: red, blue, green and yellow, as the test's.
fn quadrants() -> ImageBitmap {
    install();
    let image = ImageBitmap::new(100, 100);
    let mut canvas = Canvas::new(&image);
    canvas.draw_rect(Rect::new(0.0, 0.0, 50.0, 50.0), Paint::fill(Color::RED));
    canvas.draw_rect(Rect::new(50.0, 0.0, 100.0, 50.0), Paint::fill(Color::BLUE));
    canvas.draw_rect(Rect::new(0.0, 50.0, 50.0, 100.0), Paint::fill(Color::GREEN));
    canvas.draw_rect(Rect::new(50.0, 50.0, 100.0, 100.0), Paint::fill(Color::YELLOW));
    image
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/ImageBitmapTest.kt, testCreatedImage
#[test]
fn a_new_bitmap_keeps_what_its_made_with() {
    let image =
        ImageBitmap::new_with(10, 20, ImageBitmapConfig::Argb8888, false, &ColorSpaces::SRGB);
    assert_eq!((image.width(), image.height()), (10, 20));
    assert_eq!(image.config(), ImageBitmapConfig::Argb8888);
    assert!(!image.has_alpha());
    assert_eq!(*image.color_space(), ColorSpaces::SRGB);
    // The short form: a transparent sRGB bitmap with alpha.
    let image = ImageBitmap::new(3, 4);
    assert!(image.has_alpha());
    assert_eq!(image.config(), ImageBitmapConfig::Argb8888);
    assert_eq!(image.to_pixel_map().get(2, 3), Color::TRANSPARENT);
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/PixelMapTest.kt, testImageBitmapPixelMap
#[test]
fn a_pixel_map_reads_each_quadrant() {
    let map = quadrants().to_pixel_map();
    for (color, left, top) in
        [(Color::RED, 0, 0), (Color::BLUE, 50, 0), (Color::GREEN, 0, 50), (Color::YELLOW, 50, 50)]
    {
        for (x, y) in [(left, top), (left + 49, top), (left, top + 49), (left + 49, top + 49)] {
            assert_eq!(map.get(x, y), color, "at {x}, {y}");
        }
    }
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/PixelMapTest.kt, testImageBitmapSubsection
#[test]
fn a_pixel_map_reads_a_part_into_a_buffer_at_an_offset() {
    let image = quadrants();
    let map = image.to_pixel_map_with(48, 49, 3, 2, vec![0; 3 * 2 + 3], 3, 3);
    assert_eq!(map.get(1, 0), Color::RED);
    assert_eq!(map.get(2, 0), Color::BLUE);
    assert_eq!(map.get(1, 1), Color::GREEN);
    assert_eq!(map.get(2, 1), Color::YELLOW);
    // The offset's slots are left as they were.
    assert_eq!(&map.buffer()[..3], &[0, 0, 0]);
}

/// A 200 × 200 red bitmap, then `transform` and a blue `rect` in a save.
fn transformed(transform: impl FnOnce(&mut Canvas), rect: Rect) -> compose::ui::PixelMap {
    install();
    let image = ImageBitmap::new(200, 200);
    let mut canvas = Canvas::new(&image);
    canvas.draw_rect(Rect::new(0.0, 0.0, 200.0, 200.0), Paint::fill(Color::RED));
    canvas.with_save(|canvas| {
        transform(canvas);
        canvas.draw_rect(rect, Paint::fill(Color::BLUE));
    });
    // The transform went with the save.
    canvas.draw_rect(Rect::new(199.0, 0.0, 200.0, 1.0), Paint::fill(Color::RED));
    image.to_pixel_map()
}

#[track_caller]
fn assert_colors(map: &compose::ui::PixelMap, color: Color, points: &[(i32, i32)]) {
    for &(x, y) in points {
        assert_eq!(map.get(x, y), color, "at {x}, {y}");
    }
}

const FULL: Rect = Rect::new(0.0, 0.0, 200.0, 200.0);

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/AndroidCanvasTest.kt, testScaleWithDefaultPivot
#[test]
fn a_canvas_scales_about_its_origin() {
    let map = transformed(|canvas| canvas.scale(0.5, 0.5), FULL);
    assert_colors(&map, Color::BLUE, &[(0, 0), (99, 0), (0, 99), (99, 99)]);
    assert_colors(&map, Color::RED, &[(0, 100), (100, 0), (100, 100), (100, 99), (99, 100)]);
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/AndroidCanvasTest.kt, testScaleWithCenterPivot
#[test]
fn a_canvas_scales_about_its_center() {
    let map = transformed(|canvas| canvas.scale_around(0.5, 0.5, 100.0, 100.0), FULL);
    let (left, top, right, bottom) = (50, 50, 149, 149);
    assert_colors(&map, Color::BLUE, &[(left, top), (right, top), (left, bottom), (right, bottom)]);
    assert_colors(
        &map,
        Color::RED,
        &[
            (left - 1, top - 1),
            (left - 1, top),
            (left, top - 1),
            (right + 1, top - 1),
            (right + 1, top),
            (right, top - 1),
            (left - 1, bottom + 1),
            (left - 1, bottom),
            (left, bottom + 1),
            (right + 1, bottom + 1),
            (right + 1, bottom),
            (right, bottom + 1),
        ],
    );
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/AndroidCanvasTest.kt, testScaleWithBottomRightPivot
#[test]
fn a_canvas_scales_about_its_bottom_right() {
    let map = transformed(|canvas| canvas.scale_around(0.5, 0.5, 200.0, 200.0), FULL);
    let (left, top, right, bottom) = (100, 100, 199, 199);
    assert_colors(&map, Color::BLUE, &[(left, top), (right, top), (left, bottom), (right, bottom)]);
    assert_colors(
        &map,
        Color::RED,
        &[
            (left, top - 1),
            (left - 1, top),
            (left - 1, top - 1),
            (right, top - 1),
            (left - 1, bottom),
        ],
    );
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/AndroidCanvasTest.kt, testRotationCenterPivot
#[test]
fn a_canvas_rotates_about_its_center() {
    let map = transformed(
        |canvas| canvas.rotate_around(180.0, 100.0, 100.0),
        Rect::new(100.0, 100.0, 200.0, 200.0),
    );
    assert_colors(&map, Color::BLUE, &[(0, 0), (99, 0), (0, 99), (99, 99)]);
    assert_colors(&map, Color::RED, &[(0, 100), (100, 0), (100, 100), (100, 99), (99, 100)]);
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/AndroidCanvasTest.kt, testRotationDefaultPivot
#[test]
fn a_canvas_rotates_about_its_origin() {
    let map = transformed(|canvas| canvas.rotate(-45.0), Rect::new(0.0, 0.0, 100.0, 100.0));
    assert_colors(&map, Color::BLUE, &[(2, 0), (50, 49), (70, 0), (70, 68)]);
    assert_colors(&map, Color::RED, &[(50, 51), (75, 76)]);
}

/// An 80 × 80 white bitmap with a blue rectangle drawn through `filter`.
fn filtered(filter: ColorFilter) -> compose::ui::PixelMap {
    install();
    let image = ImageBitmap::new(80, 80);
    let mut canvas = Canvas::new(&image);
    canvas.draw_rect(Rect::new(0.0, 0.0, 80.0, 80.0), Paint::fill(Color::WHITE));
    let paint = canvas.paint_with_color_filter(Paint::fill(Color::BLUE), filter);
    canvas.draw_rect(Rect::new(0.0, 0.0, 80.0, 80.0), paint);
    image.to_pixel_map()
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/AndroidCanvasTest.kt, testColorFilterTint, testColorFilterLighting, testColorFilterColorMatrix
#[test]
fn a_canvas_draws_through_color_filters() {
    let every = |map: &compose::ui::PixelMap, color: Color| {
        for x in 0..80 {
            for y in 0..80 {
                assert_eq!(map.get(x, y), color, "at {x}, {y}");
            }
        }
    };
    every(&filtered(ColorFilter::tint_with(Color::MAGENTA, BlendMode::SrcIn)), Color::MAGENTA);
    // Blue times red, plus blue: blue.
    every(&filtered(ColorFilter::lighting(Color::RED, Color::BLUE)), Color::BLUE);
    let mut gray = ColorMatrix::new();
    gray.set_to_saturation(0.0);
    every(&filtered(ColorFilter::color_matrix(gray)), gray.transform(Color::BLUE));
}

// Compose: compose/ui/ui-graphics/src/commonMain/kotlin/androidx/compose/ui/graphics/Canvas.kt, saveLayer (no Compose test)
/// A layer gathers what's drawn in it, composited at its restore: half transparent over red.
#[test]
fn a_canvas_layer_is_composited_at_its_restore() {
    install();
    let image = ImageBitmap::new(10, 10);
    let mut canvas = Canvas::new(&image);
    canvas.draw_rect(Rect::new(0.0, 0.0, 10.0, 10.0), Paint::fill(Color::RED));
    let half = Paint::fill(Color::BLACK.with_alpha(0.5));
    canvas.save_layer(Rect::new(0.0, 0.0, 10.0, 10.0), half);
    canvas.draw_rect(Rect::new(0.0, 0.0, 10.0, 10.0), Paint::fill(Color::BLUE));
    // Not drawn yet: the layer is.
    assert_eq!(image.to_pixel_map().get(5, 5), Color::RED);
    canvas.draw_rect(Rect::new(0.0, 0.0, 5.0, 10.0), Paint::fill(Color::BLUE));
    canvas.restore();
    let blended = image.to_pixel_map().get(5, 5).to_argb();
    assert!((((blended >> 16) & 0xFF) as i32 - 0x7F).abs() <= 2, "{blended:08X}");
    assert!(((blended & 0xFF) as i32 - 0x80).abs() <= 2, "{blended:08X}");
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/drawscope/DrawScopeTest.kt, testDrawRectColor
/// A `DrawScope` draws into a bitmap's canvas, after its transform.
#[test]
fn a_draw_scope_draws_into_a_bitmap() {
    install();
    let image = ImageBitmap::new(100, 100);
    let mut canvas = Canvas::new(&image);
    canvas.translate(50.0, 0.0);
    CanvasDrawScope::draw(
        &mut canvas,
        Density::new(1.0),
        LayoutDirection::Ltr,
        Size::new(50.0, 100.0),
        |scope| {
            let bounds = scope.bounds();
            scope.draw_rect(bounds, Paint::fill(Color::RED));
        },
    );
    let map = image.to_pixel_map();
    assert_eq!(map.get(49, 50), Color::TRANSPARENT);
    assert_eq!(map.get(50, 50), Color::RED);
    assert_eq!(map.get(99, 0), Color::RED);
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/layer/AndroidGraphicsLayerTest.kt, testGraphicsLayerBitmap, testGraphicsLayerBitmapAfterDependencyReleased
#[test]
fn a_graphics_layer_gives_its_bitmap() {
    install();
    let layer = GraphicsLayer::new();
    let image = ImageBitmap::new(1, 1);
    let mut canvas = Canvas::new(&image);
    CanvasDrawScope::draw(
        &mut canvas,
        Density::new(1.0),
        LayoutDirection::Ltr,
        Size::new(1.0, 1.0),
        |scope| {
            layer.record(scope, IntSize::new(60, 40), |scope| {
                let (width, height) = (60.0, 40.0);
                let half = |left: f32, top: f32| {
                    Rect::new(left, top, left + width / 2.0, top + height / 2.0)
                };
                scope.draw_rect(half(0.0, 0.0), Paint::fill(Color::RED));
                scope.draw_rect(half(30.0, 0.0), Paint::fill(Color::BLUE));
                scope.draw_rect(half(0.0, 20.0), Paint::fill(Color::GREEN));
                scope.draw_rect(half(30.0, 20.0), Paint::fill(Color::BLACK));
            });
        },
    );
    // Where it's drawn doesn't move it in its bitmap.
    layer.set_top_left(compose::ui::IntOffset::new(7, 5));
    let bitmap = layer.to_image_bitmap();
    assert_eq!((bitmap.width(), bitmap.height()), (60, 40));
    let map = bitmap.to_pixel_map();
    assert_colors(&map, Color::RED, &[(1, 1), (28, 18)]);
    assert_colors(&map, Color::BLUE, &[(31, 1), (58, 18)]);
    assert_colors(&map, Color::GREEN, &[(1, 21), (28, 38)]);
    assert_colors(&map, Color::BLACK, &[(31, 21), (58, 38)]);
}

// Compose: compose/ui/ui-graphics/src/commonMain/kotlin/androidx/compose/ui/graphics/ImageBitmap.kt, a bitmap drawn into (no Compose test)
/// A bitmap drawn into after a renderer drew it is drawn with its new pixels: the renderer's
/// copy of the old ones isn't kept.
#[test]
fn a_bitmap_drawn_into_again_is_drawn_anew() {
    use compose::ui::platform::{DisplayList, DrawOp, FixedTextMeasurer, Renderer};
    install();
    let image = ImageBitmap::new(4, 4);
    let mut canvas = Canvas::new(&image);
    canvas.draw_rect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::RED));
    let mut list = DisplayList::default();
    let rect = Rect::new(0.0, 0.0, 4.0, 4.0);
    let draw = DrawOp::DrawImage {
        image: image.clone(),
        src: rect,
        dst: rect,
        alpha: 1.0,
        color_filter: None,
        filter_quality: compose::ui::FilterQuality::Low,
    };
    list.push(draw);
    let mut renderer = compose_skia::RasterRenderer::new(4, 4);
    renderer.render(&list, Color::WHITE, 1.0, &FixedTextMeasurer::default());
    assert_eq!(renderer.pixel(1, 1), Color::RED);
    canvas.draw_rect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::BLUE));
    renderer.render(&list, Color::WHITE, 1.0, &FixedTextMeasurer::default());
    assert_eq!(renderer.pixel(1, 1), Color::BLUE);
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/AndroidCanvasTest.kt, testDrawVerticesDoesNotCrashBelowAPI29
#[test]
fn vertices_draw_their_colors() {
    use compose::ui::canvas::{VertexMode, Vertices};
    install();
    let image = ImageBitmap::new(200, 200);
    let mut canvas = Canvas::new(&image);
    let corners = [
        Offset::new(0.0, 0.0),
        Offset::new(200.0, 0.0),
        Offset::new(0.0, 200.0),
        Offset::new(200.0, 200.0),
    ];
    let vertices = Vertices::new(
        VertexMode::Triangles,
        &corners,
        &corners,
        &[Color::RED; 4],
        &[0, 1, 2, 1, 2, 3],
    );
    canvas.draw_vertices(&Arc::new(vertices), BlendMode::SrcOver, Paint::fill(Color::BLACK));
    let map = image.to_pixel_map();
    for (x, y) in [(0, 0), (199, 0), (0, 199), (199, 199), (100, 100), (150, 20)] {
        assert_eq!(map.get(x, y), Color::RED, "at {x}, {y}");
    }
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/drawscope/DrawScopeTest.kt, testDrawPointStrokeParametersAreApplied
/// Points drawn by a `DrawScope` and by a canvas are the same: squares of the stroke's width
/// for a butt cap, as Skia's points.
#[test]
fn points_draw_alike_in_a_scope_and_on_a_canvas() {
    use compose::ui::canvas::PointMode;
    install();
    let points = [
        Offset::ZERO,
        Offset::new(10.0, 10.0),
        Offset::new(25.0, 25.0),
        Offset::new(40.0, 40.0),
        Offset::new(50.0, 50.0),
        Offset::new(75.0, 75.0),
        Offset::new(150.0, 150.0),
    ];
    let mut paint = Paint::stroke(Color::MAGENTA, 15.0);
    paint.cap = compose::ui::StrokeCap::Butt;
    let scoped = ImageBitmap::new(200, 200);
    CanvasDrawScope::draw(
        &mut Canvas::new(&scoped),
        Density::new(1.0),
        LayoutDirection::Ltr,
        Size::new(200.0, 200.0),
        |scope| scope.draw_points(&points, PointMode::Points, paint),
    );
    let direct = ImageBitmap::new(200, 200);
    Canvas::new(&direct).draw_points(PointMode::Points, &points, paint);
    assert_eq!(scoped.to_pixel_map(), direct.to_pixel_map());
    let map = direct.to_pixel_map();
    // A 15-pixel square about each point.
    assert_eq!(map.get(81, 81), Color::MAGENTA);
    assert_eq!(map.get(68, 68), Color::MAGENTA);
    assert_eq!(map.get(84, 75), Color::TRANSPARENT);
    assert_eq!(map.get(100, 100), Color::TRANSPARENT);
}

// Compose: compose/ui/ui-graphics/src/commonMain/kotlin/androidx/compose/ui/graphics/Canvas.kt, drawPoints with PointMode.Lines and Polygon (no Compose test)
#[test]
fn points_draw_as_pairs_of_lines_or_a_polygon() {
    use compose::ui::canvas::PointMode;
    install();
    let points = [Offset::new(0.0, 5.0), Offset::new(10.0, 5.0), Offset::new(10.0, 15.0)];
    let draw = |mode: PointMode| {
        let image = ImageBitmap::new(20, 20);
        Canvas::new(&image).draw_points(mode, &points, Paint::stroke(Color::RED, 2.0));
        image.to_pixel_map()
    };
    let lines = draw(PointMode::Lines);
    assert_eq!(lines.get(5, 5), Color::RED);
    // The last point has no pair.
    assert_eq!(lines.get(10, 12), Color::TRANSPARENT);
    let polygon = draw(PointMode::Polygon);
    assert_eq!((polygon.get(5, 5), polygon.get(10, 12)), (Color::RED, Color::RED));
}

// Compose: compose/ui/ui-graphics/src/commonMain/kotlin/androidx/compose/ui/graphics/Canvas.kt, skew, concat (no Compose test)
#[test]
fn a_canvas_skews_and_concatenates_matrices() {
    install();
    let skewed = ImageBitmap::new(40, 20);
    let mut canvas = Canvas::new(&skewed);
    // x' = x + y: the 10 × 10 square becomes a parallelogram leaning right.
    canvas.skew(1.0, 0.0);
    canvas.draw_rect(Rect::new(0.0, 0.0, 10.0, 10.0), Paint::fill(Color::RED));
    let map = skewed.to_pixel_map();
    assert_eq!(
        (map.get(2, 1), map.get(15, 8), map.get(2, 8)),
        (Color::RED, Color::RED, Color::TRANSPARENT)
    );

    let moved = ImageBitmap::new(40, 20);
    let mut canvas = Canvas::new(&moved);
    let mut matrix = compose::ui::Matrix::default();
    matrix.translate(20.0, 5.0, 0.0);
    canvas.concat(&matrix);
    canvas.draw_rect(Rect::new(0.0, 0.0, 10.0, 10.0), Paint::fill(Color::BLUE));
    let map = moved.to_pixel_map();
    assert_eq!(
        (map.get(21, 6), map.get(29, 14), map.get(19, 6)),
        (Color::BLUE, Color::BLUE, Color::TRANSPARENT)
    );
}

// Compose: compose/ui/ui-graphics/src/commonMain/kotlin/androidx/compose/ui/graphics/Canvas.kt, clipRect and clipPath with ClipOp.Difference (no Compose test)
#[test]
fn a_canvas_clips_out_a_rectangle() {
    use compose::ui::canvas::ClipOp;
    install();
    let image = ImageBitmap::new(30, 30);
    let mut canvas = Canvas::new(&image);
    canvas.clip_rect_with(Rect::new(10.0, 10.0, 20.0, 20.0), ClipOp::Difference);
    canvas.draw_rect(Rect::new(0.0, 0.0, 30.0, 30.0), Paint::fill(Color::GREEN));
    let map = image.to_pixel_map();
    assert_eq!((map.get(5, 5), map.get(25, 15)), (Color::GREEN, Color::GREEN));
    assert_eq!(map.get(15, 15), Color::TRANSPARENT);
}

// Compose: compose/ui/ui-graphics/src/androidMain/kotlin/androidx/compose/ui/graphics/AndroidImageBitmap.android.kt, readPixels (Android's Bitmap.getPixels unpremultiplies, rounding; no Compose test)
/// Read pixels are unpremultiplied to the nearest value, not down.
#[test]
fn read_pixels_round_to_the_nearest_value() {
    // Premultiplied: red 1 at alpha 2 is 127.5 unpremultiplied; green 50 at alpha 200, 63.75.
    let image = ImageBitmap::from_rgba_premultiplied(2, 1, vec![1, 0, 0, 2, 0, 50, 0, 200]);
    let mut pixels = [0u32; 2];
    image.read_pixels(&mut pixels, 0, 0, 2, 1, 0, 2);
    assert_eq!(pixels, [0x0280_0000, 0xC800_4000]);
}

// Compose: compose/ui/ui-graphics/src/commonMain/kotlin/androidx/compose/ui/graphics/Canvas.kt, a canvas over a bitmap (no Compose test)
/// A thousand drawings into a large bitmap are drawn in one pass when it's read, each kept.
#[test]
fn a_thousand_drawings_into_a_large_bitmap_are_drawn_at_once() {
    install();
    let image = ImageBitmap::new(1000, 1000);
    let mut canvas = Canvas::new(&image);
    let start = std::time::Instant::now();
    for index in 0..1000 {
        let (x, y) = ((index % 40) as f32 * 25.0, (index / 40) as f32 * 25.0);
        let color = if index % 2 == 0 { Color::RED } else { Color::BLUE };
        canvas.draw_rect(Rect::new(x, y, x + 25.0, y + 25.0), Paint::fill(color));
    }
    let map = image.to_pixel_map();
    eprintln!("1,000 drawings into 1000 × 1000 and a read: {:?}", start.elapsed());
    assert_eq!(map.get(12, 12), Color::RED);
    assert_eq!(map.get(37, 12), Color::BLUE);
    // The last: index 999 at column 39, row 24.
    assert_eq!(map.get(39 * 25 + 12, 24 * 25 + 12), Color::BLUE);
    assert_eq!(map.get(500, 900), Color::TRANSPARENT);
}

// Compose: compose/ui/ui-graphics/src/commonMain/kotlin/androidx/compose/ui/graphics/Canvas.kt, a canvas over a bitmap (no Compose test)
/// Two threads drawing into one bitmap lose nothing: each drawing is kept.
#[test]
fn two_threads_drawing_into_one_bitmap_keep_every_drawing() {
    install();
    let image = ImageBitmap::new(200, 2);
    std::thread::scope(|scope| {
        for row in 0..2 {
            let image = image.clone();
            scope.spawn(move || {
                let mut canvas = Canvas::new(&image);
                canvas.translate(0.0, row as f32);
                for x in 0..200 {
                    let rect = Rect::new(x as f32, 0.0, x as f32 + 1.0, 1.0);
                    canvas.draw_rect(rect, Paint::fill(Color::RED));
                    if x % 50 == 0 {
                        // A read in between draws what waits.
                        let _ = image.pixels();
                    }
                }
            });
        }
    });
    let map = image.to_pixel_map();
    for x in 0..200 {
        assert_eq!((map.get(x, 0), map.get(x, 1)), (Color::RED, Color::RED), "at {x}");
    }
}

// Compose: compose/ui/ui-graphics/src/commonMain/kotlin/androidx/compose/ui/graphics/Canvas.kt, drawImage (no Compose test)
/// A bitmap drawn into itself draws its pixels as they were when it was drawn; two canvases'
/// transforms stay their own.
#[test]
fn a_bitmap_drawn_into_itself_reads_its_pixels_then() {
    install();
    let image = ImageBitmap::new(4, 2);
    let mut canvas = Canvas::new(&image);
    let mut moved = Canvas::new(&image);
    moved.translate(2.0, 0.0);
    canvas.draw_rect(Rect::new(0.0, 0.0, 2.0, 2.0), Paint::fill(Color::RED));
    // Copies the red half to the right, through the moved canvas.
    moved.draw_image(&image, compose::ui::Offset::ZERO, Paint::fill(Color::BLACK));
    // Then the left half turns blue, after the copy.
    canvas.draw_rect(Rect::new(0.0, 0.0, 2.0, 2.0), Paint::fill(Color::BLUE));
    let map = image.to_pixel_map();
    let colors = (map.get(1, 1), map.get(2, 1), map.get(3, 0));
    assert_eq!(colors, (Color::BLUE, Color::RED, Color::RED));
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/drawscope/DrawScopeTest.kt, testBrushResetOnSubsequentDrawWithAlphaBitmap
#[test]
fn draw_scope_resets_brush_when_drawing_alpha8_bitmap() {
    use compose::ui::{Brush, PaintStyle};

    install();
    let width = 200u32;
    let height = 200u32;
    let brush = Brush::horizontal_gradient([Color::TRANSPARENT, Color::BLUE, Color::TRANSPARENT]);
    let mask = ImageBitmap::new_with(
        width / 2,
        height / 2,
        ImageBitmapConfig::Alpha8,
        true,
        &ColorSpaces::SRGB,
    );
    {
        let mut mask_canvas = Canvas::new(&mask);
        mask_canvas
            .draw_rect(Rect::new(0.0, 0.0, width as f32, height as f32), Paint::fill(Color::GREEN));
    }
    let filter = ColorFilter::tint(Color::RED);
    let draw_scene = |out: &ImageBitmap| {
        let mut canvas = Canvas::new(out);
        CanvasDrawScope::draw(
            &mut canvas,
            Density::new(1.0),
            LayoutDirection::Ltr,
            Size::new(width as f32, height as f32),
            |scope| {
                let paint = scope.brush_paint(&brush, 1.0, PaintStyle::Fill);
                scope.draw_rect(scope.bounds(), paint);
                scope.draw_image_filtered(
                    &mask,
                    Rect::new(0.0, 0.0, (width / 2) as f32, (height / 2) as f32),
                    Rect::new(
                        width as f32 / 4.0,
                        height as f32 / 4.0,
                        width as f32 * 0.75,
                        height as f32 * 0.75,
                    ),
                    1.0,
                    Some(&filter),
                );
            },
        );
    };
    let out = ImageBitmap::new(width, height);
    draw_scene(&out);
    let map = out.to_pixel_map();
    // Outside mask: gradient corners transparent, middle blue; inside mask: tinted red, not gradient.
    assert!(map.get(0, 10).alpha() <= 0.02, "{:?}", map.get(0, 10));
    let top_mid = map.get(width as i32 / 2, 10);
    assert_eq!((top_mid.to_argb() & 0x00FF_FFFF), 0x0000_00FF, "{top_mid:?}");
    assert!(top_mid.alpha() >= 0.99, "{top_mid:?}");
    assert_eq!(map.get(width as i32 / 2, height as i32 / 2), Color::RED);
}

#[test]
fn bitmap_configs_and_color_spaces_round_trip() {
    install();
    // Alpha8 preserves alpha and snapshots cleanly.
    let alpha = ImageBitmap::new_with(4, 4, ImageBitmapConfig::Alpha8, true, &ColorSpaces::SRGB);
    Canvas::new(&alpha)
        .draw_rect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::from_argb(0x80FF0000)));
    assert_eq!(alpha.pixels().len(), 16);
    assert!((alpha.color(1, 1).alpha() - 0.5).abs() < 0.02);
    assert_eq!(alpha.to_pixel_map().get(1, 1), Color::from_argb(0x80000000));

    // Rgb565 draws opaque 16-bit RGB colors.
    let rgb565 = ImageBitmap::new_with(4, 4, ImageBitmapConfig::Rgb565, false, &ColorSpaces::SRGB);
    Canvas::new(&rgb565).draw_rect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::RED));
    assert_eq!(rgb565.pixels().len(), 32);
    assert_eq!(rgb565.color(2, 2), Color::RED);

    // DisplayP3 Argb8888 keeps P3 values in P3 space and draws accurately onto an sRGB canvas.
    let p3_red = Color::new_with(1.0, 0.0, 0.0, 1.0, &ColorSpaces::DISPLAY_P3);
    let p3_bmp =
        ImageBitmap::new_with(4, 4, ImageBitmapConfig::Argb8888, true, &ColorSpaces::DISPLAY_P3);
    Canvas::new(&p3_bmp).draw_rect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(p3_red));
    let got = p3_bmp.color(1, 1);
    assert_eq!(*got.color_space(), ColorSpaces::DISPLAY_P3);
    assert!((got.red() - 1.0).abs() < 0.02 && got.green().abs() < 0.02, "{got:?}");

    // Drawing the DisplayP3 bitmap onto another canvas preserves its red tint without shift.
    let target = ImageBitmap::new(4, 4);
    Canvas::new(&target).draw_image(&p3_bmp, Offset::ZERO, Paint::fill(Color::BLACK));
    assert_eq!(target.to_pixel_map().get(1, 1), p3_red.convert(&ColorSpaces::SRGB));
}
