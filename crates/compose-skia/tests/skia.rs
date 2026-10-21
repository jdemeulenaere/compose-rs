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
