//! The Skia measurer and renderer, with the system's fonts.

use compose::material3::light_color_scheme;
use compose::test::*;
use compose::ui::{
    Color, Constraints, Density, IntSize, Offset, TextStyle,
    platform::{Renderer, TextLayoutOptions, TextMeasurer, UiHost},
};
use compose_skia::{RasterRenderer, SkiaTextMeasurer};
use compose_ui::DEFAULT_SHADOW_COLOR;

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
    host.with_tree(|tree| {
        renderer.render(tree.display_list(), Color::WHITE, 1.0, tree.text_measurer_ref())
    });
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
    let mut list = compose::ui::platform::DisplayList::default();
    list.push(compose::ui::platform::DrawOp::DrawRect(
        compose::ui::Rect::new(0.0, 0.0, 4.0, 8.0),
        compose::ui::Paint::fill(Color::RED),
    ));
    source.render(&list, Color::WHITE, 1.0, &compose::ui::platform::FixedTextMeasurer::default());
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
            modifier = Modifier.size((16.dp(), 16.dp())),
            content_scale = compose::foundation::ContentScale::FillBounds,
        );
    }
    let mut host =
        UiHost::new(IntSize::new(32, 32), Density::new(1.0), Box::new(SkiaTextMeasurer::new()));
    host.set_content(move || Shown(image.clone()));
    host.frame();
    let mut renderer = RasterRenderer::new(32, 32);
    host.with_tree(|tree| {
        renderer.render(tree.display_list(), Color::BLUE, 1.0, tree.text_measurer_ref())
    });
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
                .shadow_with(
                    12.dp(),
                    RECTANGLE_SHAPE,
                    false,
                    DEFAULT_SHADOW_COLOR,
                    DEFAULT_SHADOW_COLOR,
                )
                .background(Color::WHITE)
                .size(100.dp()),
            Alignment::TOP_START,
            false,
            compose::runtime::callback(|_| {}),
        );
    });
    host.frame();
    let mut renderer = RasterRenderer::new(200, 200);
    host.with_tree(|tree| {
        renderer.render(tree.display_list(), Color::WHITE, 1.0, tree.text_measurer_ref())
    });
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
    ui.host().with_tree(|tree| {
        renderer.render(tree.display_list(), Color::WHITE, 1.0, tree.text_measurer_ref())
    });
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

/// Two overlapping squares (red, then blue over its right half) in a layer at half alpha under
/// `strategy`, drawn with `optimizations`: the pixel where they overlap, and one of the red
/// alone.
fn overlapping_pixels(
    strategy: compose::ui::CompositingStrategy,
    optimizations: compose::ui::platform::Optimizations,
) -> (Color, Color) {
    use compose::ui::{GraphicsLayerScope, Modifier, Paint, Rect, UnitExt};
    let mut host =
        UiHost::new(IntSize::new(40, 20), Density::new(1.0), Box::new(SkiaTextMeasurer::new()));
    host.set_optimizations(optimizations);
    host.set_content(move || {
        compose::foundation::layout::Canvas(
            Modifier.size((40.dp(), 20.dp())).graphics_layer_with(GraphicsLayerScope {
                alpha: 0.5,
                compositing_strategy: strategy,
                ..GraphicsLayerScope::default()
            }),
            |scope| {
                scope.draw_rect(Rect::new(0.0, 0.0, 30.0, 20.0), Paint::fill(Color::RED));
                scope.draw_rect(Rect::new(20.0, 0.0, 40.0, 20.0), Paint::fill(Color::BLUE));
            },
        );
    });
    host.frame();
    let mut renderer = RasterRenderer::new(40, 20);
    renderer.set_optimizations(host.optimizations());
    host.with_tree(|tree| {
        renderer.render(tree.display_list(), Color::WHITE, 1.0, tree.text_measurer_ref())
    });
    (renderer.pixel(25, 10), renderer.pixel(5, 10))
}

/// `Auto` and `Offscreen` fade the layer's content as one (the overlap shows no red, as
/// Compose Desktop's offscreen buffer); `ModulateAlpha` fades each drawing (the red shows
/// through the blue).
#[test]
fn compositing_strategies_fade_as_compose_desktops() {
    use compose::ui::CompositingStrategy;
    use compose::ui::platform::Optimizations;
    let (auto, auto_red) = overlapping_pixels(CompositingStrategy::Auto, Optimizations::NONE);
    let (offscreen, _) = overlapping_pixels(CompositingStrategy::Offscreen, Optimizations::NONE);
    let (modulated, modulated_red) =
        overlapping_pixels(CompositingStrategy::ModulateAlpha, Optimizations::NONE);
    let near = |a: u8, b: u8| a.abs_diff(b) <= 2;
    // Blue at half alpha over white.
    assert!(near(auto.red(), 0x7F) && near(auto.blue(), 0xFF), "{auto:?}");
    assert_eq!(offscreen, auto);
    // Blue at half alpha over red at half alpha over white: the red took its green away.
    assert!(modulated.green() + 30 < auto.green(), "{modulated:?} against {auto:?}");
    // One drawing fades the same either way (but for rounding).
    assert!(
        near(modulated_red.green(), auto_red.green()) && near(modulated_red.red(), auto_red.red()),
        "{modulated_red:?} against {auto_red:?}"
    );
}

/// With `SINGLE_DRAWING_FADE`, an `Auto` layer over two drawings still composites offscreen
/// (they may overlap): the same pixels as without it.
#[test]
fn the_single_drawing_fade_keeps_overlapping_layers_offscreen() {
    use compose::ui::CompositingStrategy;
    use compose::ui::platform::{Optimization, Optimizations};
    let fade = Optimizations::NONE.with(Optimization::SINGLE_DRAWING_FADE, true);
    assert_eq!(
        overlapping_pixels(CompositingStrategy::Auto, fade),
        overlapping_pixels(CompositingStrategy::Auto, Optimizations::NONE)
    );
}

/// A 20 × 20 layer at half alpha drawing one rect twice its width (not clipped), with
/// `optimizations`: the pixels inside the layer and past it.
fn overflowing_pixels(optimizations: compose::ui::platform::Optimizations) -> (Color, Color) {
    use compose::ui::{GraphicsLayerScope, Modifier, Paint, Rect, UnitExt};
    let mut host =
        UiHost::new(IntSize::new(40, 20), Density::new(1.0), Box::new(SkiaTextMeasurer::new()));
    host.set_optimizations(optimizations);
    host.set_content(move || {
        compose::foundation::layout::Canvas(
            Modifier.size((20.dp(), 20.dp())).graphics_layer_with(GraphicsLayerScope {
                alpha: 0.5,
                ..GraphicsLayerScope::default()
            }),
            |scope| scope.draw_rect(Rect::new(0.0, 0.0, 40.0, 20.0), Paint::fill(Color::RED)),
        );
    });
    host.frame();
    let mut renderer = RasterRenderer::new(40, 20);
    renderer.set_optimizations(host.optimizations());
    host.with_tree(|tree| {
        renderer.render(tree.display_list(), Color::WHITE, 1.0, tree.text_measurer_ref())
    });
    (renderer.pixel(10, 10), renderer.pixel(30, 10))
}

/// With `SINGLE_DRAWING_FADE`, a drawing past its layer's bounds isn't faded by its paint: the
/// offscreen buffer cuts it, the same pixels as without the flag (D648).
#[test]
fn the_single_drawing_fade_keeps_its_layers_bounds() {
    use compose::ui::platform::{Optimization, Optimizations};
    let fade = Optimizations::NONE.with(Optimization::SINGLE_DRAWING_FADE, true);
    let (inside, past) = overflowing_pixels(Optimizations::NONE);
    assert_eq!(past, Color::WHITE, "the offscreen buffer bounds it");
    assert_ne!(inside, Color::WHITE);
    assert_eq!(overflowing_pixels(fade), (inside, past));
}

/// Every pixel of a 40 × 40 window holding a 20 × 20 layer at half alpha, moved and turned by
/// `layer`, its one rect filling it, drawn with `optimizations`.
fn transformed_layer_pixels(
    layer: compose::ui::GraphicsLayerScope,
    optimizations: compose::ui::platform::Optimizations,
) -> Vec<Color> {
    layer_pixels(layer, optimizations, |scope| {
        scope.draw_rect(
            compose::ui::Rect::new(0.0, 0.0, 20.0, 20.0),
            compose::ui::Paint::fill(Color::RED),
        )
    })
}

/// Every pixel of a 40 x 40 window holding a 20 x 20 layer at half alpha, moved, scaled and
/// turned by `layer`, drawing `draw`, with `optimizations`.
fn layer_pixels(
    layer: compose::ui::GraphicsLayerScope,
    optimizations: compose::ui::platform::Optimizations,
    draw: fn(&mut compose::ui::DrawScope<'_>),
) -> Vec<Color> {
    use compose::foundation::layout::Box;
    use compose::ui::{Alignment, GraphicsLayerScope, Modifier, UnitExt};
    let mut host =
        UiHost::new(IntSize::new(40, 40), Density::new(1.0), Box::new(SkiaTextMeasurer::new()));
    host.set_optimizations(optimizations);
    host.set_content(move || {
        Box(
            Modifier.size(40.dp()),
            Alignment::CENTER,
            false,
            compose::runtime::callback(move |_| {
                compose::foundation::layout::Canvas(
                    Modifier
                        .size((20.dp(), 20.dp()))
                        .graphics_layer_with(GraphicsLayerScope { alpha: 0.5, ..layer }),
                    draw,
                );
            }),
        );
    });
    host.frame();
    let mut renderer = RasterRenderer::new(40, 40);
    renderer.set_optimizations(host.optimizations());
    host.with_tree(|tree| {
        renderer.render(tree.display_list(), Color::WHITE, 1.0, tree.text_measurer_ref())
    });
    (0..40).flat_map(|y| (0..40).map(move |x| (x, y))).map(|(x, y)| renderer.pixel(x, y)).collect()
}

/// With `SINGLE_DRAWING_FADE`, a drawing filling its layer is faded by its paint and keeps its
/// anti-aliased edges, at fractional moves and turned: the same pixels as offscreen, but for
/// rounding (2/255 a channel). A clip to the layer's bounds instead (D648's first version) lost
/// up to 64/255 at those edges.
#[test]
fn the_single_drawing_fade_keeps_a_moved_or_turned_layers_edges() {
    use compose::ui::GraphicsLayerScope;
    use compose::ui::platform::{Optimization, Optimizations};
    let fade = Optimizations::NONE.with(Optimization::SINGLE_DRAWING_FADE, true);
    let near = |a: u8, b: u8| a.abs_diff(b) <= 2;
    let layers = [
        GraphicsLayerScope { translation_x: 0.3, translation_y: 0.3, ..Default::default() },
        GraphicsLayerScope { translation_x: 0.5, translation_y: 0.5, ..Default::default() },
        GraphicsLayerScope { translation_x: 0.7, translation_y: 0.7, ..Default::default() },
        GraphicsLayerScope { rotation_z: 30.0, ..Default::default() },
    ];
    for layer in layers {
        let offscreen = transformed_layer_pixels(layer, Optimizations::NONE);
        let faded = transformed_layer_pixels(layer, fade);
        assert!(offscreen.iter().any(|pixel| *pixel != Color::WHITE));
        for (index, (a, b)) in offscreen.iter().zip(&faded).enumerate() {
            assert!(
                near(a.red(), b.red()) && near(a.green(), b.green()) && near(a.blue(), b.blue()),
                "{layer:?}: pixel {index}: {b:?} against {a:?}"
            );
        }
    }
}

/// A clip is a layer clipping to its shape, anti-aliased as Compose Desktop's render node's
/// `setClipRRect`: a corner outside the rounded rectangle shows the background.
#[test]
fn a_clip_clips_to_its_rounded_outline() {
    use compose::foundation::RoundedCornerShape;
    use compose::foundation::layout::Box;
    use compose::ui::{Alignment, Modifier, UnitExt};
    let mut host =
        UiHost::new(IntSize::new(40, 40), Density::new(1.0), Box::new(SkiaTextMeasurer::new()));
    host.set_content(|| {
        Box(
            Modifier.size(40.dp()).clip(RoundedCornerShape(12.dp())).background(Color::RED),
            Alignment::TOP_START,
            false,
            compose::runtime::callback(|_| {}),
        );
    });
    host.frame();
    let mut renderer = RasterRenderer::new(40, 40);
    host.with_tree(|tree| {
        renderer.render(tree.display_list(), Color::WHITE, 1.0, tree.text_measurer_ref())
    });
    assert_eq!(renderer.pixel(1, 1), Color::WHITE, "outside the corner");
    assert_eq!(renderer.pixel(20, 20), Color::RED);
    assert_eq!(renderer.pixel(20, 1), Color::RED, "an edge's middle");
}

/// Review: a `ModulateAlpha` layer (alpha 0.5) around a child layer drawn `Offscreen` holding
/// two overlapping squares. The child's offscreen buffer is drawn at full alpha, then faded as
/// one by the outer alpha (HWUI: a non-overlapping node's alpha multiplies its child node's
/// layer alpha, not the drawings inside that layer): the overlap is blue at half alpha over
/// white, no red through it.
#[test]
fn review_a_modulated_alpha_fades_a_nested_offscreen_layer_as_one() {
    use compose::foundation::layout::Box;
    use compose::ui::{
        Alignment, CompositingStrategy, GraphicsLayerScope, Modifier, Paint, Rect, UnitExt,
    };
    let mut host =
        UiHost::new(IntSize::new(40, 20), Density::new(1.0), Box::new(SkiaTextMeasurer::new()));
    host.set_content(move || {
        Box(
            Modifier.size((40.dp(), 20.dp())).graphics_layer_with(GraphicsLayerScope {
                alpha: 0.5,
                compositing_strategy: CompositingStrategy::ModulateAlpha,
                ..GraphicsLayerScope::default()
            }),
            Alignment::TOP_START,
            false,
            compose::runtime::callback(|_| {
                compose::foundation::layout::Canvas(
                    Modifier.size((40.dp(), 20.dp())).graphics_layer_with(GraphicsLayerScope {
                        compositing_strategy: CompositingStrategy::Offscreen,
                        ..GraphicsLayerScope::default()
                    }),
                    |scope| {
                        scope.draw_rect(Rect::new(0.0, 0.0, 30.0, 20.0), Paint::fill(Color::RED));
                        scope.draw_rect(Rect::new(20.0, 0.0, 40.0, 20.0), Paint::fill(Color::BLUE));
                    },
                );
            }),
        );
    });
    host.frame();
    let mut renderer = RasterRenderer::new(40, 20);
    host.with_tree(|tree| {
        renderer.render(tree.display_list(), Color::WHITE, 1.0, tree.text_measurer_ref())
    });
    let overlap = renderer.pixel(25, 10);
    let near = |a: u8, b: u8| a.abs_diff(b) <= 2;
    assert!(
        near(overlap.red(), 0x7F) && near(overlap.green(), 0x7F) && near(overlap.blue(), 0xFF),
        "{overlap:?}: the red shows through the offscreen layer"
    );
}

/// With `SINGLE_DRAWING_FADE`, the same pixels as without, but for rounding (2/255 a channel):
/// strokes stay offscreen (thin, Skia draws them as hairlines, their joints blended twice where
/// the buffer blends once, 15 to 20 of 255 apart folded; a 2 px stroked rounded rect's corners
/// were 5 apart), an inverted rect past the bounds too (sorted, it reaches x 30: 128 apart
/// folded before D648 sorted it), and fills within the bounds fold, at scale 1, 0.5 and turned.
#[test]
fn the_single_drawing_fade_matches_offscreen() {
    use compose::ui::platform::{Optimization, Optimizations};
    use compose::ui::{DrawScope, GraphicsLayerScope, Offset, Paint, Rect};
    let fade = Optimizations::NONE.with(Optimization::SINGLE_DRAWING_FADE, true);
    let near = |a: u8, b: u8| a.abs_diff(b) <= 2;
    fn round_rect(scope: &mut DrawScope<'_>, width: f32) {
        let paint = Paint::stroke(Color::RED, width);
        scope.draw_round_rect(Rect::new(3.0, 3.0, 17.0, 17.0), [5.0; 4], paint);
    }
    fn circle(scope: &mut DrawScope<'_>, width: f32) {
        scope.draw_circle(Offset::new(10.0, 10.0), 7.0, Paint::stroke(Color::RED, width));
    }
    fn rect(scope: &mut DrawScope<'_>, width: f32) {
        scope.draw_rect(Rect::new(3.5, 3.5, 16.5, 16.5), Paint::stroke(Color::RED, width));
    }
    fn inverted(scope: &mut DrawScope<'_>) {
        scope.draw_rect(Rect::new(30.0, 5.0, 10.0, 15.0), Paint::fill(Color::RED));
    }
    fn round_fill(scope: &mut DrawScope<'_>) {
        scope.draw_round_rect(Rect::new(3.0, 3.0, 17.0, 17.0), [5.0; 4], Paint::fill(Color::RED));
    }
    fn circle_fill(scope: &mut DrawScope<'_>) {
        scope.draw_circle(Offset::new(10.0, 10.0), 7.0, Paint::fill(Color::RED));
    }
    type Case = (&'static str, f32, fn(&mut DrawScope<'_>));
    let cases: [Case; 17] = [
        ("inverted rect", 1.0, inverted),
        ("round rect fill", 1.0, round_fill),
        ("round rect fill at 0.5", 0.5, round_fill),
        ("circle fill", 1.0, circle_fill),
        ("circle fill at 0.5", 0.5, circle_fill),
        ("round rect 0", 1.0, |scope| round_rect(scope, 0.0)),
        ("round rect 0.5", 1.0, |scope| round_rect(scope, 0.5)),
        ("round rect 1", 1.0, |scope| round_rect(scope, 1.0)),
        ("round rect 2", 1.0, |scope| round_rect(scope, 2.0)),
        ("circle 0", 1.0, |scope| circle(scope, 0.0)),
        ("circle 0.5", 1.0, |scope| circle(scope, 0.5)),
        ("circle 1", 1.0, |scope| circle(scope, 1.0)),
        ("rect 0", 1.0, |scope| rect(scope, 0.0)),
        ("rect 0.5", 1.0, |scope| rect(scope, 0.5)),
        ("rect 1", 1.0, |scope| rect(scope, 1.0)),
        ("hairline at 0.5", 0.5, |scope| round_rect(scope, 0.0)),
        ("1.5 at 0.5", 0.5, |scope| round_rect(scope, 1.5)),
    ];
    for (name, scale, draw) in cases {
        let layer = GraphicsLayerScope { scale_x: scale, scale_y: scale, ..Default::default() };
        let offscreen = layer_pixels(layer, Optimizations::NONE, draw);
        let faded = layer_pixels(layer, fade, draw);
        assert!(offscreen.iter().any(|pixel| *pixel != Color::WHITE), "{name}");
        for (index, (a, b)) in offscreen.iter().zip(&faded).enumerate() {
            assert!(
                near(a.red(), b.red()) && near(a.green(), b.green()) && near(a.blue(), b.blue()),
                "{name}: pixel {index}: {b:?} against {a:?}"
            );
        }
    }
}
