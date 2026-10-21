//! The Skia measurer and renderer, with the system's fonts.

use compose::material3::light_color_scheme;
use compose::test::*;
use compose::ui::{
    Color, Constraints, Density, IntSize, Offset, Renderer, TextLayoutOptions, TextMeasurer,
    TextStyle, UiHost,
};
use compose_skia::{RasterRenderer, SkiaTextMeasurer};

#[test]
fn paragraphs_measure_wrap_and_hit_test() {
    let mut measurer = SkiaTextMeasurer::new();
    let style = TextStyle::DEFAULT;
    let options = TextLayoutOptions::default();
    let density = Density::default();
    let one = measurer.layout("Hello world", &style, options, Constraints::unbounded(), &density);
    let metrics = measurer.metrics(one);
    assert!(metrics.size.width > 30 && metrics.size.height > 10, "{metrics:?}");
    assert_eq!(metrics.line_count, 1);
    let narrow = measurer.layout(
        "Hello world",
        &style,
        options,
        Constraints::at_most(metrics.size.width / 2, 1000),
        &density,
    );
    assert_eq!(measurer.metrics(narrow).line_count, 2);
    // Hit testing and cursors move with the text.
    let start = measurer.cursor_rect(one, 0);
    let middle = measurer.cursor_rect(one, 5);
    let end = measurer.cursor_rect(one, "Hello world".len());
    assert!(start.left < middle.left && middle.left < end.left, "{start:?} {middle:?} {end:?}");
    assert_eq!(measurer.hit_test(one, Offset::new(middle.left + 0.5, 5.0)).offset, 5);
    // Density scales text.
    let doubled = measurer.layout(
        "Hello world",
        &style,
        options,
        Constraints::unbounded(),
        &Density::new(2.0),
    );
    assert!(measurer.metrics(doubled).size.width >= metrics.size.width * 2 - 2);
    for handle in [one, narrow, doubled] {
        measurer.release(handle);
    }
    assert_eq!(measurer.live(), 0);
    let empty = measurer.layout("", &style, options, Constraints::unbounded(), &density);
    assert!(measurer.cursor_rect(empty, 0).height() > 0.0);
}

#[test]
fn the_counter_renders_with_skia() {
    let mut host =
        UiHost::new(IntSize::new(320, 200), Density::new(2.0), Box::new(SkiaTextMeasurer::new()));
    host.set_content(counter::Counter);
    host.frame();
    let mut renderer = RasterRenderer::new(640, 400);
    let list = host.display_list();
    host.with_tree(|tree| renderer.render(&list, Color::WHITE, 1.0, tree.text_measurer()));
    // The first button's fill, in the theme's primary color, at its left edge (16 dp padding,
    // then the pill's straight part).
    let button = light_color_scheme().primary;
    assert_eq!(renderer.pixel(2 * 16 + 30, 2 * 16 + 2 * 20), button);
    assert_eq!(renderer.pixel(2, 2), Color::WHITE);
    let png = renderer.png();
    assert!(png.starts_with(&[0x89, b'P', b'N', b'G']), "a PNG");
}

#[test]
fn the_test_rule_runs_with_the_skia_measurer() {
    let mut ui = compose_test_with(
        IntSize::new(400, 300),
        Density::default(),
        Box::new(SkiaTextMeasurer::new()),
        counter::Counter,
    );
    ui.on_node(has_text_containing("Count")).perform_click();
    ui.on_node_with_text("Count: 1").assert_exists();
}

#[test]
fn images_decode_and_draw_with_skia() {
    use compose::foundation::ImageBitmap;
    use compose::runtime::composable;
    use compose::ui::{Modifier, UnitExt};
    // A PNG of a red square on white, encoded by the renderer.
    let mut source = RasterRenderer::new(8, 8);
    let mut list = compose::ui::DisplayList::default();
    list.push(compose::ui::DrawOp::DrawRect(
        compose::ui::Rect::new(0.0, 0.0, 4.0, 8.0),
        compose::ui::Paint::fill(Color::RED),
    ));
    source.render(&list, Color::WHITE, 1.0, &compose::ui::FixedTextMeasurer::default());
    let png = source.png();
    let image = compose_skia::decode_image(&png).expect("a PNG decodes");
    assert_eq!((image.width(), image.height()), (8, 8));
    assert_eq!(&image.pixels()[0..4], &[255, 0, 0, 255], "red at the left");
    assert_eq!(&image.pixels()[7 * 4..8 * 4], &[255, 255, 255, 255], "white at the right");
    assert!(compose_skia::decode_image(b"not an image").is_none());
    // Drawn twice as large (FillBounds) by `Image`, at density 1.
    #[composable]
    #[allow(non_snake_case)]
    fn Shown(image: ImageBitmap) {
        compose::foundation::Image!(
            image,
            None,
            modifier = Modifier.size_wh(16.dp(), 16.dp()),
            content_scale = compose::foundation::ContentScale::FillBounds,
        );
    }
    let mut host =
        UiHost::new(IntSize::new(32, 32), Density::new(1.0), Box::new(SkiaTextMeasurer::new()));
    host.set_content(move || Shown(image.clone()));
    host.frame();
    let mut renderer = RasterRenderer::new(32, 32);
    let list = host.display_list();
    host.with_tree(|tree| renderer.render(&list, Color::BLUE, 1.0, tree.text_measurer()));
    assert_eq!(renderer.pixel(2, 8), Color::RED);
    assert_eq!(renderer.pixel(13, 8), Color::WHITE);
    assert_eq!(renderer.pixel(20, 20), Color::BLUE, "outside the image");
}

#[test]
fn shadows_darken_beneath_an_elevated_shape() {
    use compose::foundation::RECTANGLE_SHAPE;
    use compose::foundation::layout::Box;
    use compose::ui::{Alignment, Modifier, UnitExt};
    let mut host =
        UiHost::new(IntSize::new(200, 200), Density::new(1.0), Box::new(SkiaTextMeasurer::new()));
    host.set_content(|| {
        Box(
            Modifier
                .padding(50.dp())
                .shadow(12.dp(), RECTANGLE_SHAPE, false)
                .background(Color::WHITE)
                .size(100.dp()),
            Alignment::TOP_START,
            false,
            compose::runtime::callback(|_| {}),
        );
    });
    host.frame();
    let mut renderer = RasterRenderer::new(200, 200);
    let list = host.display_list();
    host.with_tree(|tree| renderer.render(&list, Color::WHITE, 1.0, tree.text_measurer()));
    // Under the shape's bottom edge (the light is above), darker than the background; far
    // away, untouched.
    let below = renderer.pixel(100, 153);
    assert!(below.red() < 250, "the shadow under the shape: {below:?}");
    assert_eq!(renderer.pixel(100, 100), Color::WHITE, "the shape itself");
    assert_eq!(renderer.pixel(2, 2), Color::WHITE, "far from it");
}
