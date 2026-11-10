//! Drop and inner shadows drawn by Skia, as Compose's `DropShadowTest`, `InnerShadowTest` and
//! their painters' tests check them.

#![allow(non_snake_case)]

use compose::foundation::CIRCLE_SHAPE;
use compose::foundation::layout::*;
use compose::runtime::*;
use compose::test::*;
use compose::ui::shadow::{DropShadowPainter, InnerShadowPainter, Shadow};
use compose::ui::{
    Alignment, Brush, Color, ColorFilter, ContentScale, CornerSize, Dp, DpOffset, IntSize,
    Modifier, Painter, Shape, UnitExt,
};

const TAG: &str = "shadowed";

/// A 100 px white node with a 40 px red box in its middle casting `shadow` of `shape`.
fn dropped(shape: Shape, shadow: Shadow) -> CapturedImage {
    dropped_on(Color::WHITE, shape, shadow)
}

/// A 100 px node of `background` with a 40 px red box in its middle casting `shadow` of
/// `shape`.
fn dropped_on(background: Color, shape: Shape, shadow: Shadow) -> CapturedImage {
    set_capture_renderer(compose_skia::raster_colors);
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        let (shape, shadow) = (shape.clone(), shadow.clone());
        Box!(
            modifier = Modifier.test_tag(TAG).size(100.dp()).background(background),
            content_alignment = Alignment::CENTER,
            {
                Box!(
                    modifier = Modifier
                        .size(40.dp())
                        .drop_shadow(shape.clone(), shadow.clone())
                        .background(Color::RED)
                );
            }
        );
    });
    ui.on_node_with_tag(TAG).capture_to_image()
}

/// How dark a pixel is: 0 white, 255 black.
fn darkness(color: Color) -> u8 {
    255 - ((color.to_argb() >> 8) as u8)
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/DropShadowTest.kt, dropShadowDrawn
#[test]
fn a_drop_shadow_darkens_around_its_shape() {
    let image = dropped(Shape::Rectangle, Shadow::new(Dp(8.0)));
    image.assert_pixel_color(Color::RED, 50, 50);
    image.assert_pixel_color(Color::WHITE, 2, 2);
    let near = darkness(image.pixel(28, 50));
    assert!(near > 30 && near < 255, "the blurred edge: {}", near);
    assert!(darkness(image.pixel(20, 50)) < near, "fading out");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/DropShadowScreenShotTest.kt, dropShadowSpread, dropShadowOffset
#[test]
fn a_drop_shadow_spreads_and_moves() {
    let plain = dropped(Shape::Rectangle, Shadow::new(Dp(4.0)));
    let spread = dropped(Shape::Rectangle, Shadow { spread: Dp(6.0), ..Shadow::new(Dp(4.0)) });
    assert!(darkness(spread.pixel(25, 50)) > darkness(plain.pixel(25, 50)));
    let moved = dropped(
        Shape::Rectangle,
        Shadow { offset: DpOffset::new(Dp(8.0), Dp(0.0)), ..Shadow::new(Dp(4.0)) },
    );
    assert!(darkness(moved.pixel(73, 50)) > darkness(moved.pixel(27, 50)));
}

/// A circle's shadow follows the circle: the box's corner, outside it, stays light.
#[test]
fn a_drop_shadow_follows_its_shape() {
    let image = dropped(CIRCLE_SHAPE, Shadow::new(Dp(4.0)));
    let (edge, corner) = (darkness(image.pixel(50, 27)), darkness(image.pixel(28, 28)));
    assert!(edge as u16 > corner as u16 + 20, "{edge} {corner}");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/DropShadowScreenShotTest.kt, dropShadowGradientBrush
#[test]
fn a_drop_shadow_is_painted_by_its_brush() {
    let brush = Brush::horizontal_gradient([Color::RED, Color::BLUE]);
    let image =
        dropped(Shape::Rectangle, Shadow { spread: Dp(4.0), ..Shadow::brushed(Dp(6.0), brush) });
    let (left, right) = (image.pixel(27, 50), image.pixel(73, 50));
    assert!(((left.to_argb() >> 16) as u8) > (left.to_argb() as u8), "{left:?}");
    assert!((right.to_argb() as u8) > ((right.to_argb() >> 16) as u8), "{right:?}");
}

/// A 60 px white box with `shadow` inside its rectangle.
fn inner(shadow: Shadow) -> CapturedImage {
    inner_of(Shape::Rectangle, shadow)
}

/// A 60 px white box with `shadow` inside `shape`.
fn inner_of(shape: Shape, shadow: Shadow) -> CapturedImage {
    set_capture_renderer(compose_skia::raster_colors);
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        Box!(
            modifier = Modifier
                .test_tag(TAG)
                .size(60.dp())
                .background(Color::WHITE)
                .inner_shadow(shape.clone(), shadow.clone())
        );
    });
    ui.on_node_with_tag(TAG).capture_to_image()
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/InnerShadowTest.kt, innerShadowDrawn
#[test]
fn an_inner_shadow_darkens_inside_its_edges() {
    let image = inner(Shadow::new(Dp(8.0)));
    let edge = darkness(image.pixel(1, 30));
    assert!(edge > 60, "{edge}");
    assert!(darkness(image.pixel(8, 30)) < edge, "fading in");
    image.assert_pixel_color(Color::WHITE, 30, 30);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/InnerShadowScreenShotTest.kt, innerShadowSpread, innerShadowOffset
#[test]
fn an_inner_shadow_spreads_inward_and_moves() {
    let plain = inner(Shadow::new(Dp(4.0)));
    let spread = inner(Shadow { spread: Dp(8.0), ..Shadow::new(Dp(4.0)) });
    assert!(darkness(spread.pixel(8, 30)) > darkness(plain.pixel(8, 30)) + 50);
    let moved = inner(Shadow { offset: DpOffset::new(Dp(8.0), Dp(0.0)), ..Shadow::new(Dp(4.0)) });
    assert!(darkness(moved.pixel(4, 30)) > darkness(moved.pixel(56, 30)) + 50);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/DropShadowTest.kt, dropShadowRedrawnOnChange
#[test]
fn a_new_shadow_draws_again() {
    set_capture_renderer(compose_skia::raster_colors);
    let color = mutable_state_of(Color::BLACK);
    let handle = color.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        Box!(
            modifier = Modifier.test_tag(TAG).size(100.dp()).background(Color::WHITE),
            content_alignment = Alignment::CENTER,
            {
                Box!(
                    modifier = Modifier
                        .size(40.dp())
                        .drop_shadow(Shape::Rectangle, Shadow::colored(Dp(4.0), handle.get()))
                        .background(Color::RED)
                );
            }
        );
    });
    let before = ui.on_node_with_tag(TAG).capture_to_image().pixel(28, 50);
    color.set(Color::BLUE);
    ui.wait_for_idle();
    let after = ui.on_node_with_tag(TAG).capture_to_image().pixel(28, 50);
    assert!(
        (after.to_argb() as u8) > ((after.to_argb() >> 16) as u8)
            && (before.to_argb() as u8) == ((before.to_argb() >> 16) as u8),
        "{before:?} {after:?}"
    );
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/shadow/DropShadowPainterTest.kt, testDropShadowPainterAlpha, testDropShadowPainterColorFilter
// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/shadow/InnerShadowPainterTest.kt, testInnerShadowPainterColor
#[test]
fn the_shadow_painters_take_their_alpha_and_filter() {
    set_capture_renderer(compose_skia::raster_colors);
    let painted = |painter: Painter, alpha: f32, filter: Option<ColorFilter>| {
        let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
            Box!(
                modifier =
                    Modifier.test_tag(TAG).size(60.dp()).background(Color::WHITE).paint_with(
                        painter.clone(),
                        true,
                        Alignment::CENTER,
                        ContentScale::FillBounds,
                        alpha,
                        filter
                    )
            );
        });
        ui.on_node_with_tag(TAG).capture_to_image()
    };
    let inset = |shadow: Shadow| Painter::new(InnerShadowPainter::new(Shape::Rectangle, shadow));
    let opaque = painted(inset(Shadow::new(Dp(6.0))), 1.0, None).pixel(1, 30);
    let faded = painted(inset(Shadow::new(Dp(6.0))), 0.5, None).pixel(1, 30);
    assert!(darkness(faded) < darkness(opaque), "{faded:?} {opaque:?}");
    let tinted = painted(inset(Shadow::new(Dp(6.0))), 1.0, Some(ColorFilter::tint(Color::BLUE)));
    let edge = tinted.pixel(1, 30);
    assert!((edge.to_argb() as u8) > ((edge.to_argb() >> 16) as u8) + 50, "{edge:?}");
    let dropped = Painter::new(DropShadowPainter::new(CIRCLE_SHAPE, Shadow::new(Dp(4.0))));
    painted(dropped, 1.0, None).assert_pixel_color(Color::BLACK, 30, 30);
}

/// An inner shadow stays inside its shape: a circle's leaves the box's corners, outside the
/// circle, and what's around the box as they were.
#[test]
fn an_inner_shadow_stays_inside_its_shape() {
    set_capture_renderer(compose_skia::raster_colors);
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        Box!(
            modifier = Modifier.test_tag(TAG).size(100.dp()).background(Color::WHITE),
            content_alignment = Alignment::CENTER,
            {
                Box!(
                    modifier = Modifier
                        .size(60.dp())
                        .background(Color::WHITE)
                        .inner_shadow(CIRCLE_SHAPE, Shadow::new(Dp(8.0)))
                );
            }
        );
    });
    let image = ui.on_node_with_tag(TAG).capture_to_image();
    let edge = darkness(image.pixel(21, 50));
    assert!(edge > 60, "inside the circle's edge: {edge}");
    for (x, y) in [(21, 21), (78, 78), (18, 50), (50, 18), (2, 2)] {
        image.assert_pixel_color(Color::WHITE, x, y);
    }
}

/// A path's shape: a rectangle with its corners cut by 2 px (`Outline::Generic`).
fn cut() -> Shape {
    Shape::Cut([CornerSize::Px(2.0); 4])
}

/// A translucent shadow of a path spread past it is one mask faded, as Compose's tinted
/// bitmap: half its alpha, or a color of half alpha, is half as dark as the opaque one, where
/// the spread's band and the blurred fill overlap.
#[test]
fn a_translucent_path_shadow_with_a_spread_fades_as_one() {
    let shadow = Shadow { spread: Dp(4.0), ..Shadow::new(Dp(6.0)) };
    let opaque = dropped(cut(), shadow.clone());
    let faded = dropped(cut(), shadow.clone().with_alpha(0.5));
    let half_black = Shadow { color: Color::BLACK.with_alpha(0.5), ..shadow };
    let tinted = dropped(cut(), half_black);
    // The box spans 30 to 70; the spread reaches 26.
    for x in [22, 25, 28] {
        let full = darkness(opaque.pixel(x, 50)) as i32;
        for image in [&faded, &tinted] {
            let half = darkness(image.pixel(x, 50)) as i32;
            assert!((half - full / 2).abs() <= 2, "at {x}: {half} against {full}");
        }
    }
}

/// An inner shadow of a path spread into it fades as one mask too.
#[test]
fn a_translucent_inner_path_shadow_with_a_spread_fades_as_one() {
    let shadow = Shadow { spread: Dp(4.0), ..Shadow::new(Dp(6.0)) };
    let opaque = inner_of(cut(), shadow.clone());
    let faded = inner_of(cut(), shadow.with_alpha(0.5));
    for x in [1, 4, 7] {
        let full = darkness(opaque.pixel(x, 30)) as i32;
        let half = darkness(faded.pixel(x, 30)) as i32;
        assert!(full > 40, "at {x}: {full}");
        assert!((half - full / 2).abs() <= 2, "at {x}: {half} against {full}");
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/DropShadowScreenShotTest.kt, dropShadowGradientBrush
/// A drop shadow's brush is resolved for the shadow's whole extent, as Compose's for its
/// bitmap: the box grown by the radius and the spread each side, from its top left moved back
/// by them. Over black, a horizontal gradient's blue share across it grows with x from there.
#[test]
fn a_drop_shadows_brush_spans_the_shadow() {
    let brush = Brush::horizontal_gradient([Color::RED, Color::BLUE]);
    let shadow = Shadow { spread: Dp(4.0), ..Shadow::brushed(Dp(6.0), brush) };
    let image = dropped_on(Color::BLACK, Shape::Rectangle, shadow);
    // The box spans 30 to 70; the shadow's bitmap 20 to 80.
    for x in [24, 27, 73, 76] {
        let pixel = image.pixel(x, 50);
        let (red, blue) = (((pixel.to_argb() >> 16) as u8) as f32, (pixel.to_argb() as u8) as f32);
        let share = blue / (red + blue);
        let expected = (x as f32 + 0.5 - 20.0) / 60.0;
        assert!((share - expected).abs() < 0.04, "at {x}: {share} against {expected}");
    }
}

fn image_shader_shadow_brush() -> Brush {
    use compose::ui::{ImageBitmap, Paint, Rect, Shader, ShaderBrush, TileMode};
    compose::ui::platform::set_bitmap_rasterizer(std::sync::Arc::new(
        compose_skia::SkiaBitmapRasterizer,
    ));
    let bitmap = ImageBitmap::new(100, 100);
    let mut canvas = compose::ui::canvas::Canvas::new(&bitmap);
    canvas.draw_rect(Rect::new(0.0, 0.0, 100.0, 50.0), Paint::fill(Color::RED));
    canvas.draw_rect(Rect::new(0.0, 50.0, 100.0, 100.0), Paint::fill(Color::GREEN));
    Brush::from(ShaderBrush::new(Shader::image(&bitmap, TileMode::Clamp, TileMode::Clamp)))
}

struct RectPathShape;

impl compose::ui::CustomShape for RectPathShape {
    fn create_outline(
        &self,
        size: compose::ui::Size,
        _layout_direction: compose::ui::LayoutDirection,
        _density: &compose::ui::Density,
    ) -> compose::ui::Outline {
        let path = compose::ui::Path::new();
        path.line_to(size.width, 0.0);
        path.line_to(size.width, size.height);
        path.line_to(0.0, size.height);
        path.close();
        compose::ui::Outline::Generic(path.data(), compose::ui::Offset::ZERO)
    }

    fn same(&self, other: &dyn compose::ui::CustomShape) -> bool {
        other.downcast_ref::<RectPathShape>().is_some()
    }
}

fn paint_drop_shadow(
    shape: Shape,
    shadow: Shadow,
    alpha: f32,
    color_filter: Option<ColorFilter>,
) -> CapturedImage {
    set_capture_renderer(compose_skia::raster_colors);
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        let painter = Painter::new(DropShadowPainter::new(shape.clone(), shadow.clone()));
        Box!(
            modifier = Modifier.test_tag(TAG).size(100.dp()).background(Color::BLUE).paint_with(
                painter,
                true,
                Alignment::CENTER,
                ContentScale::FillBounds,
                alpha,
                color_filter
            )
        );
    });
    ui.on_node_with_tag(TAG).capture_to_image()
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/shadow/DropShadowPainterTest.kt, testDropShadowPainterWithBrush, testDropShadowPainterPathWithBrush, testDropShadowPainterWithBrushAndColorFilter, testDropShadowPainterWithBrushAndAlpha
#[test]
fn drop_shadow_painter_with_image_shader_brush_and_filters() {
    let channel = |c: Color, shift: u32| ((c.to_argb() >> shift) & 0xff) as i32;

    // 1. Rectangle + ImageShader brush (top half red, bottom half green over blue; shadow
    // extent shifts brush origin back by reach = 20, so top split is at y=30):
    let rect_img = paint_drop_shadow(
        Shape::Rectangle,
        Shadow::brushed(Dp(20.0), image_shader_shadow_brush()),
        1.0,
        None,
    );
    assert!(channel(rect_img.pixel(50, 20), 16) >= channel(rect_img.pixel(50, 5), 16));
    assert!(channel(rect_img.pixel(50, 75), 8) >= channel(rect_img.pixel(50, 95), 8));
    assert!(channel(rect_img.pixel(50, 20), 16) > 50 && channel(rect_img.pixel(50, 75), 8) > 50);

    // 2. Outline::Generic path + ImageShader brush:
    let path_img = paint_drop_shadow(
        Shape::custom(RectPathShape),
        Shadow::brushed(Dp(20.0), image_shader_shadow_brush()),
        1.0,
        None,
    );
    assert!(channel(path_img.pixel(50, 20), 16) >= channel(path_img.pixel(50, 5), 16));
    assert!(channel(path_img.pixel(50, 75), 8) >= channel(path_img.pixel(50, 95), 8));
    assert!(channel(path_img.pixel(50, 20), 16) > 50 && channel(path_img.pixel(50, 75), 8) > 50);

    // 3. ImageShader brush + ColorFilter::tint(Color::RED) makes even bottom green side red:
    let tinted = paint_drop_shadow(
        Shape::Rectangle,
        Shadow::brushed(Dp(20.0), image_shader_shadow_brush()),
        1.0,
        Some(ColorFilter::tint(Color::RED)),
    );
    assert!(channel(tinted.pixel(50, 60), 16) > channel(tinted.pixel(50, 60), 8) + 50);

    // 4. Vertical gradient brush + alpha = 0.5 fades over blue background:
    let gradient = Brush::vertical_gradient([
        (0.0, Color::RED),
        (0.5, Color::RED),
        (0.5, Color::GREEN),
        (1.0, Color::GREEN),
    ]);
    let faded = paint_drop_shadow(Shape::Rectangle, Shadow::brushed(Dp(20.0), gradient), 0.5, None);
    assert!(channel(faded.pixel(50, 40), 16) <= 204);
    assert!(channel(faded.pixel(50, 40), 0) > 0);
    assert!(channel(faded.pixel(50, 60), 8) <= 204);
    assert!(channel(faded.pixel(50, 60), 0) > 0);
}
