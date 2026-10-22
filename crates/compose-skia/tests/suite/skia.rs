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

#[test]
fn text_styles_change_the_layout_as_compose() {
    use compose::ui::{FontFamily, FontStyle, Sp, TextAlign, TextOverflow};
    let mut measurer = SkiaTextMeasurer::new();
    let density = Density::default();
    let options = TextLayoutOptions::default();
    let unbounded = Constraints::unbounded();
    let width = |measurer: &mut SkiaTextMeasurer, style: TextStyle, options| {
        let paragraph = measurer.layout("Hello world", &style, options, unbounded, &density);
        let width = measurer.metrics(paragraph).size.width;
        measurer.release(paragraph);
        width
    };
    let plain = width(&mut measurer, TextStyle::DEFAULT, options);
    // Letter spacing: 2 px more per character (11 of them).
    let spaced = TextStyle { letter_spacing: Sp(2.0), ..TextStyle::DEFAULT };
    assert!((width(&mut measurer, spaced, options) - plain - 22).abs() <= 2);
    // Another family or slant measures differently.
    let mono = TextStyle { font_family: Some(FontFamily::Monospace), ..TextStyle::DEFAULT };
    assert_ne!(width(&mut measurer, mono, options), plain);
    let italic = TextStyle { font_style: Some(FontStyle::Italic), ..TextStyle::DEFAULT };
    let _ = width(&mut measurer, italic, options);
    // An ellipsis keeps one line in the width, without soft wrap.
    let narrow = Constraints::at_most(plain / 2, 1000);
    let ellipsis = TextLayoutOptions {
        soft_wrap: false,
        overflow: TextOverflow::Ellipsis,
        ..TextLayoutOptions::default()
    };
    let cut = measurer.layout("Hello world", &TextStyle::DEFAULT, ellipsis, narrow, &density);
    let metrics = measurer.metrics(cut);
    assert_eq!(metrics.line_count, 1);
    assert!(metrics.size.width <= plain / 2, "{metrics:?}");
    measurer.release(cut);
    // Centered in a fixed width: the first character starts in the middle.
    let centered = TextStyle { text_align: Some(TextAlign::Center), ..TextStyle::DEFAULT };
    let fixed = Constraints::fixed(plain * 3, 100);
    let paragraph = measurer.layout("Hello world", &centered, options, fixed, &density);
    let start = measurer.cursor_rect(paragraph, 0);
    assert!((start.left - plain as f32).abs() <= 2.0, "{start:?}");
    measurer.release(paragraph);
}

#[test]
fn annotated_text_draws_each_span_in_its_style() {
    use compose::material3::AnnotatedText;
    use compose::ui::{AnnotatedString, SpanStyle};
    let text = AnnotatedString::build(|builder| {
        builder.append("aaaa");
        builder.with_style(
            SpanStyle { color: Color::RED, background: Color::BLUE, ..SpanStyle::DEFAULT },
            |builder| {
                builder.append("bbbb");
            },
        );
    });
    let mut ui = compose_test_with(
        IntSize::new(300, 100),
        Density::default(),
        Box::new(SkiaTextMeasurer::new()),
        move || {
            AnnotatedText(
                text.clone(),
                compose::ui::Modifier,
                Color::BLACK,
                compose::ui::TextOverflow::Clip,
                true,
                u32::MAX,
                1,
                None,
                TextStyle::DEFAULT,
            )
        },
    );
    ui.on_node_with_text("aaaabbbb").assert_exists();
    let mut renderer = RasterRenderer::new(300, 100);
    let list = ui.host().display_list();
    ui.host().with_tree(|tree| renderer.render(&list, Color::WHITE, 1.0, tree.text_measurer()));
    let mut colors = std::collections::HashSet::new();
    for y in 0..40 {
        for x in 0..300 {
            colors.insert(renderer.pixel(x, y));
        }
    }
    assert!(colors.contains(&Color::BLUE), "the span's background");
    assert!(colors.contains(&Color::BLACK), "the first run's glyphs");
    assert!(colors.contains(&Color::RED), "the span's glyphs");
}
