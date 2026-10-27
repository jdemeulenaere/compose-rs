//! Borders and the corner-based and generic shapes, drawn by Skia: Compose's `BorderTest`,
//! `BorderRendererTest` and the shapes' clips, checked pixel by pixel.

use compose::foundation::layout::Box as Container;
use compose::foundation::shape::*;
use compose::ui::{
    Alignment, BorderStroke, Brush, Color, Density, IntSize, LayoutDirection, Modifier, Shape,
    TileMode, UnitExt,
    platform::{Renderer, UiHost},
};
use compose_skia::{RasterRenderer, SkiaTextMeasurer};

/// Renders `content` in a 40 x 40 window at density 1 over white.
fn render(content: impl Fn() + 'static) -> RasterRenderer {
    let mut host = UiHost::new(
        IntSize::new(40, 40),
        Density::new(1.0),
        std::boxed::Box::new(SkiaTextMeasurer::new()),
    );
    host.set_content(content);
    host.frame();
    let mut renderer = RasterRenderer::new(40, 40);
    host.with_tree(|tree| {
        renderer.render(tree.display_list(), Color::WHITE, 1.0, tree.text_measurer_ref())
    });
    renderer
}

/// A 40 x 40 box with a blue background (a rectangle, as Compose's test) and `modifier` after.
fn boxed(modifier: Modifier) -> RasterRenderer {
    render(move || {
        Container(
            Modifier.size(40.dp()).background(Color::BLUE).then(modifier.clone()),
            Alignment::TOP_START,
            false,
            compose::runtime::callback(|_| {}),
        );
    })
}

/// The shapes of Compose's `BorderTest`: a rectangle, a circle, simple and non-simple rounded
/// corners, and a generic path.
fn shapes() -> Vec<(&'static str, Shape)> {
    vec![
        ("Rectangle", Shape::Rectangle),
        ("Circle", CIRCLE_SHAPE),
        ("Rounded", RoundedCornerShape(5.0)),
        ("NonSimpleRounded", RoundedCornerShape((5, 10, 15, 20))),
        (
            "Generic",
            GenericShape(|path, size, _| {
                path.add_rect(compose::ui::Rect::new(0.0, 0.0, size.width, size.height))
            }),
        ),
    ]
}

fn reddish(color: Color) -> bool {
    color.red() > 200 && color.blue() < 60
}

fn bluish(color: Color) -> bool {
    color.blue() > 200 && color.red() < 60
}

/// A 10 px border inside each shape: red at the edge's middle, blue in the middle.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BorderTest.kt, border_color
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BorderTest.kt, border_brush
#[test]
fn a_border_is_drawn_inside_each_shape() {
    for (name, shape) in shapes() {
        for stroke in [
            BorderStroke::new(10.dp(), Color::RED),
            BorderStroke::new(10.dp(), Brush::Solid(Color::RED)),
        ] {
            let mut renderer = boxed(Modifier.border_stroke(stroke, shape.clone()));
            assert!(bluish(renderer.pixel(20, 20)), "{name}: middle {:?}", renderer.pixel(20, 20));
            for (x, y) in [(5, 20), (34, 20), (20, 5), (20, 34)] {
                assert!(
                    reddish(renderer.pixel(x, y)),
                    "{name}: ({x}, {y}) {:?}",
                    renderer.pixel(x, y)
                );
            }
        }
    }
}

/// A gradient border: its colors where the gradient puts them.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BorderTest.kt, border_shaderBrush
#[test]
fn a_gradient_border_paints_its_gradient() {
    let brush = Brush::horizontal_gradient_with(
        [Color::RED, Color::GREEN],
        0.0,
        f32::INFINITY,
        TileMode::Clamp,
    );
    for (name, shape) in shapes() {
        let mut renderer = boxed(Modifier.border_with(10.dp(), brush.clone(), shape));
        let (left, right) = (renderer.pixel(3, 20), renderer.pixel(36, 20));
        assert!(left.red() > left.green(), "{name}: left {left:?}");
        assert!(right.green() > right.red(), "{name}: right {right:?}");
        assert!(bluish(renderer.pixel(20, 20)), "{name}");
    }
}

/// A border wider than half the box fills the shape.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BorderTest.kt, border_biggerThanLayout_fills
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BorderRendererTest.kt, drawBorder_biggerThanBounds_fills
#[test]
fn a_border_wider_than_the_box_fills_the_shape() {
    for (name, shape) in shapes() {
        let mut renderer = boxed(Modifier.border_with(1500.dp(), Color::RED, shape));
        for (x, y) in [(20, 20), (5, 20), (20, 34)] {
            assert!(reddish(renderer.pixel(x, y)), "{name}: ({x}, {y}) {:?}", renderer.pixel(x, y));
        }
    }
}

/// A border exactly half the box's side is a stroke, not a fill (Compose fills only beyond).
#[test]
fn a_border_of_half_the_side_strokes() {
    let mut renderer = boxed(Modifier.border_with(20.dp(), Color::RED, Shape::Rectangle));
    for (x, y) in [(20, 20), (1, 1), (38, 38)] {
        assert!(reddish(renderer.pixel(x, y)), "({x}, {y}) {:?}", renderer.pixel(x, y));
    }
}

/// A negative or unspecified width draws nothing.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BorderTest.kt, border_lessThanZero_doesNothing
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BorderRendererTest.kt, drawBorder_negativeWidth_drawsNothing
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BorderRendererTest.kt, drawBorder_unspecifiedWidth_drawsNothing
#[test]
fn a_negative_or_unspecified_width_draws_nothing() {
    for (name, shape) in shapes() {
        for width in [(-5.0).dp(), compose::ui::Dp::UNSPECIFIED] {
            let mut renderer = boxed(Modifier.border_with(width, Color::RED, shape.clone()));
            for (x, y) in [(20, 20), (1, 20), (20, 1)] {
                assert!(bluish(renderer.pixel(x, y)), "{name} {width:?}: ({x}, {y})");
            }
        }
    }
}

/// A zero-sized box draws no border.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BorderTest.kt, border_zeroSizeLayout_drawsNothing
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BorderRendererTest.kt, drawBorder_zeroSizeBounds_drawsNothing
#[test]
fn a_zero_sized_box_draws_no_border() {
    for (name, shape) in shapes() {
        let mut renderer = render(move || {
            Container(
                Modifier.size((0.dp(), 40.dp())).border_with(4.dp(), Color::RED, shape.clone()),
                Alignment::TOP_START,
                false,
                compose::runtime::callback(|_| {}),
            );
        });
        for y in 0..40 {
            for x in 0..40 {
                assert_eq!(renderer.pixel(x, y), Color::WHITE, "{name}: ({x}, {y})");
            }
        }
    }
}

/// A hairline is one pixel; a fractional width rounds up to whole pixels.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BorderRendererTest.kt, drawBorder_hairlineWidth_drawsOnePixelBorder
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BorderRendererTest.kt, drawBorder_fractionalWidth_roundsUpToWholePixels
#[test]
fn a_hairline_is_one_pixel_and_fractions_round_up() {
    let mut renderer =
        boxed(Modifier.border_with(compose::ui::Dp::HAIRLINE, Color::RED, Shape::Rectangle));
    assert!(reddish(renderer.pixel(0, 20)) && bluish(renderer.pixel(1, 20)));
    let mut renderer = boxed(Modifier.border_with(2.4.dp(), Color::RED, Shape::Rectangle));
    assert!(reddish(renderer.pixel(2, 20)) && bluish(renderer.pixel(3, 20)));
}

/// A border's shape is laid out in the layout direction: a cut corner's border mirrors.
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/BorderTest.kt, border_rtl_initially
#[test]
fn a_border_follows_the_layout_direction() {
    let content = |direction: LayoutDirection| {
        render(move || {
            compose::runtime::CompositionLocalProvider(
                compose::ui::LOCAL_LAYOUT_DIRECTION.provides(direction),
                || {
                    Container(
                        Modifier.size(40.dp()).border_with(
                            2.dp(),
                            Color::RED,
                            CutCornerShape((20.0, 0.0, 0.0, 0.0)),
                        ),
                        Alignment::TOP_START,
                        false,
                        compose::runtime::callback(|_| {}),
                    );
                },
            );
        })
    };
    // Left to right the top left is cut: its corner pixel is outside; right to left, the top
    // right.
    let mut ltr = content(LayoutDirection::Ltr);
    assert_eq!(ltr.pixel(1, 1), Color::WHITE);
    assert!(reddish(ltr.pixel(38, 1)));
    let mut rtl = content(LayoutDirection::Rtl);
    assert_eq!(rtl.pixel(38, 1), Color::WHITE);
    assert!(reddish(rtl.pixel(1, 1)));
}

/// Clipping to a cut, absolute or generic shape: what's outside the path isn't drawn.
#[test]
fn a_clip_to_a_path_shape_keeps_what_is_inside() {
    let triangle = GenericShape(|path, size, _| {
        path.move_to(0.0, 0.0);
        path.line_to(size.width, 0.0);
        path.line_to(0.0, size.height);
        path.close();
    });
    // Each shape with points outside it.
    for (shape, outside) in [
        (CutCornerShape(10.0), [(39, 39), (0, 39)]),
        (AbsoluteCutCornerShape(10.0), [(39, 39), (0, 39)]),
        (triangle, [(39, 39), (25, 25)]),
    ] {
        let drawn = shape.clone();
        let mut renderer = render(move || {
            Container(
                Modifier.size(40.dp()).clip(drawn.clone()).background(Color::RED),
                Alignment::TOP_START,
                false,
                compose::runtime::callback(|_| {}),
            );
        });
        assert!(reddish(renderer.pixel(12, 12)), "{shape:?}: {:?}", renderer.pixel(12, 12));
        for (x, y) in outside {
            assert_eq!(renderer.pixel(x, y), Color::WHITE, "{shape:?} ({x}, {y})");
        }
    }
}

/// A shadow of a cut shape is drawn under it (Compose Desktop's path shadow).
#[test]
fn a_cut_shapes_shadow_is_drawn() {
    let mut renderer = render(|| {
        Container(
            Modifier.size(40.dp()),
            Alignment::CENTER,
            false,
            compose::runtime::callback(|_| {
                Container(
                    Modifier
                        .size(20.dp())
                        .shadow_with(8.dp(), CutCornerShape(5.0), false, Color::BLACK, Color::BLACK)
                        .background_with(Color::WHITE, CutCornerShape(5.0)),
                    Alignment::TOP_START,
                    false,
                    compose::runtime::callback(|_| {}),
                );
            }),
        );
    });
    let below = renderer.pixel(20, 32);
    assert!(below != Color::WHITE, "a shadow under the box: {below:?}");
}
