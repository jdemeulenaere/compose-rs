//! Adversarial review tests for `w2-graphics` (`D2400`–`D2406`, `D2430`–`D2433`):
//! `TextNode` hot-path `.paragraph()` avoidance, empty & multi-paragraph `CustomShaderBrush`
//! resolution in `Observation::Draw`, `Canvas::save_layer` `Shader::Image` snapshotting,
//! non-`Argb8888` / opaque `F16` / wide-gamut `ImageBitmap` round-trips, `ModulateAlpha` &
//! `SINGLE_DRAWING_FADE` shader replay, and `FilterQuality` updates across `Image` & `BitmapIcon`.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use compose::foundation::*;
use compose::material3::BitmapIcon;
use compose::runtime::*;
use compose::test::*;
use compose::ui::canvas::{Canvas, CanvasDrawScope};
use compose::ui::colorspace::{ColorSpace, ColorSpaces};
use compose::ui::platform::{
    FixedTextMeasurer, Optimization, Optimizations, ParagraphHandle, ParagraphMetrics, Renderer,
    TextLayoutOptions, TextMeasurer as PlatformTextMeasurer, TextPosition, set_bitmap_rasterizer,
};
use compose::ui::{
    AnnotatedRange, AnnotatedString, Annotation, BlendMode, Brush, Color, CompositingStrategy,
    Constraints, CustomShaderBrush, Density, FilterQuality, GraphicsLayerScope, ImageBitmap,
    ImageBitmapConfig, IntSize, LayoutDirection, Matrix, Modifier, Offset, Paint, PaintStyle,
    Paragraph, ParagraphStyle, Rect, Shader, ShaderBrush, Shadow, Size, SpanStyle, TileMode,
    UnitExt,
};
use compose_skia::{RasterRenderer, SkiaBitmapRasterizer, SkiaTextMeasurer};

const TAG: &str = "TAG";

fn install_skia() {
    set_bitmap_rasterizer(Arc::new(SkiaBitmapRasterizer));
    set_capture_renderer(compose_skia::raster_colors);
}

struct CountingMeasurer {
    inner: FixedTextMeasurer,
    paragraph_calls: Rc<Cell<usize>>,
    height_calls: Rc<Cell<usize>>,
}

impl PlatformTextMeasurer for CountingMeasurer {
    fn layout(
        &mut self,
        text: &str,
        style: &TextStyle,
        options: TextLayoutOptions,
        constraints: Constraints,
        density: &Density,
    ) -> ParagraphHandle {
        self.inner.layout(text, style, options, constraints, density)
    }

    fn metrics(&self, paragraph: ParagraphHandle) -> ParagraphMetrics {
        self.inner.metrics(paragraph)
    }

    fn height(&self, paragraph: ParagraphHandle) -> f32 {
        self.height_calls.set(self.height_calls.get() + 1);
        self.inner.height(paragraph)
    }

    fn hit_test(&self, paragraph: ParagraphHandle, position: Offset) -> TextPosition {
        self.inner.hit_test(paragraph, position)
    }

    fn cursor_rect(&self, paragraph: ParagraphHandle, offset: usize) -> Rect {
        self.inner.cursor_rect(paragraph, offset)
    }

    fn paragraph(&self, paragraph: ParagraphHandle) -> Rc<dyn Paragraph> {
        self.paragraph_calls.set(self.paragraph_calls.get() + 1);
        self.inner.paragraph(paragraph)
    }

    fn text(&self, paragraph: ParagraphHandle) -> &str {
        self.inner.text(paragraph)
    }

    fn release(&mut self, paragraph: ParagraphHandle) {
        self.inner.release(paragraph);
    }
}

#[derive(Clone)]
struct StateSplitBrush {
    flipped: State<bool>,
    calls: Arc<AtomicUsize>,
}

impl CustomShaderBrush for StateSplitBrush {
    fn create_shader(&self, size: Size) -> Shader {
        assert!(
            size.width > 0.0 && size.height > 0.0,
            "CustomShaderBrush::create_shader called on empty size: {size:?}"
        );
        self.calls.fetch_add(1, Ordering::Relaxed);
        let (top, bottom) = if self.flipped.get() {
            (Color::GREEN, Color::YELLOW)
        } else {
            (Color::RED, Color::BLUE)
        };
        Shader::linear_gradient(
            Offset::ZERO,
            Offset::new(0.0, size.height),
            [(0.0, top), (0.5, top), (0.5, bottom), (1.0, bottom)],
            TileMode::Clamp,
        )
    }

    fn same(&self, other: &dyn CustomShaderBrush) -> bool {
        other.downcast_ref::<StateSplitBrush>().is_some()
    }
}

/// `TextNode::measure` must never invoke `TextMeasurer::paragraph()` unless `on_text_layout` is
/// attached, and must not query `height()` when the text has no dynamic `CustomShaderBrush`.
#[test]
fn text_measure_skips_paragraph_queries_unless_callback_attached() {
    let paragraph_calls = Rc::new(Cell::new(0));
    let height_calls = Rc::new(Cell::new(0));
    let style_state =
        mutable_state_of(TextStyle { color: Color::RED, font_size: 16.sp(), ..TextStyle::DEFAULT });
    let style_handle = style_state.as_state();
    let flipped = mutable_state_of(false);
    let custom_calls = Arc::new(AtomicUsize::new(0));
    let dynamic_brush: Brush = ShaderBrush::custom(StateSplitBrush {
        flipped: flipped.as_state(),
        calls: custom_calls.clone(),
    })
    .into();

    let mut ui = compose_test_with(
        IntSize::new(300, 100),
        Density::new(1.0),
        std::boxed::Box::new(CountingMeasurer {
            inner: FixedTextMeasurer::default(),
            paragraph_calls: paragraph_calls.clone(),
            height_calls: height_calls.clone(),
        }),
        move || {
            BasicText!("Plain label", style = style_handle.get());
        },
    );
    ui.wait_for_idle();
    assert_eq!(paragraph_calls.get(), 0, "plain text measure must not call paragraph()");
    assert_eq!(height_calls.get(), 0, "plain text measure must not call height()");

    // Predefined gradient & fixed `ShaderBrush` are non-dynamic: still 0 `paragraph()` or `height()`.
    style_state.set(TextStyle {
        brush: Some(Brush::horizontal_gradient([Color::RED, Color::BLUE])),
        font_size: 16.sp(),
        ..TextStyle::DEFAULT
    });
    ui.wait_for_idle();
    style_state.set(TextStyle {
        brush: Some(Brush::from(Shader::linear_gradient(
            Offset::ZERO,
            Offset::new(50.0, 0.0),
            [Color::RED, Color::BLUE],
            TileMode::Clamp,
        ))),
        font_size: 16.sp(),
        ..TextStyle::DEFAULT
    });
    ui.wait_for_idle();
    assert_eq!(paragraph_calls.get(), 0);
    assert_eq!(height_calls.get(), 0);

    // Switching to a dynamic `CustomShaderBrush` queries `height()` (O(1)), still 0 `paragraph()`.
    style_state.set(TextStyle {
        brush: Some(dynamic_brush),
        font_size: 16.sp(),
        ..TextStyle::DEFAULT
    });
    ui.wait_for_idle();
    assert_eq!(paragraph_calls.get(), 0, "dynamic shader must use height(), not paragraph()");
    assert!(height_calls.get() >= 1);
    assert_eq!(custom_calls.load(Ordering::Relaxed), 1);
}

#[test]
fn custom_shader_brush_skips_empty_text_and_resolves_multi_paragraph_spans() {
    install_skia();

    // 1. Empty text `""` has zero width: `create_shader` must NOT be called (as `ShaderBrushSpan`).
    let flipped = mutable_state_of(false);
    let empty_calls = Arc::new(AtomicUsize::new(0));
    let empty_brush: Brush = ShaderBrush::custom(StateSplitBrush {
        flipped: flipped.as_state(),
        calls: empty_calls.clone(),
    })
    .into();
    let mut empty_ui = compose_test_with(
        IntSize::new(200, 100),
        Density::new(1.0),
        std::boxed::Box::new(SkiaTextMeasurer::new()),
        move || {
            BasicText!(
                "",
                style = TextStyle {
                    brush: Some(empty_brush.clone()),
                    font_family: Some(basic_measure_font().to_font_family()),
                    font_size: 40.sp(),
                    ..TextStyle::DEFAULT
                },
            );
        },
    );
    empty_ui.wait_for_idle();
    assert_eq!(empty_calls.load(Ordering::Relaxed), 0, "empty text must not invoke create_shader");

    // 2. Multi-paragraph `AnnotatedString` (`ParagraphStyle` on each paragraph) where the SECOND
    // paragraph ("██") carries a `SpanStyle` `CustomShaderBrush` splitting top half vs bottom half:
    // its `create_shader(size)` must receive that paragraph's own `(width, height)`, not the whole
    // `MultiParagraph`'s height, and toggling state inside `create_shader` updates in draw without
    // recomposing!
    let span_flipped = mutable_state_of(false);
    let span_calls = Arc::new(AtomicUsize::new(0));
    let span_brush: Brush = ShaderBrush::custom(StateSplitBrush {
        flipped: span_flipped.as_state(),
        calls: span_calls.clone(),
    })
    .into();
    // Sample font renders 'I'/'H'/'A' as full-em solid blocks!
    let para_str = "HH\nHH";
    let annotated = AnnotatedString::with_annotations(
        para_str,
        vec![
            AnnotatedRange::new(Annotation::ParagraphStyle(ParagraphStyle::default()), 0, 3),
            AnnotatedRange::new(Annotation::ParagraphStyle(ParagraphStyle::default()), 3, 5),
            AnnotatedRange::new(
                Annotation::SpanStyle(SpanStyle::DEFAULT.with_brush(Some(span_brush), 1.0)),
                3,
                5,
            ),
        ],
    );
    let recompositions = Rc::new(Cell::new(0usize));
    let recompositions_clone = recompositions.clone();
    let mut multi_ui = compose_test_with(
        IntSize::new(200, 200),
        Density::new(1.0),
        std::boxed::Box::new(SkiaTextMeasurer::new()),
        move || {
            recompositions_clone.set(recompositions_clone.get() + 1);
            BasicAnnotatedText!(
                annotated.clone(),
                style = TextStyle {
                    color: Color::WHITE,
                    font_family: Some(basic_measure_font().to_font_family()),
                    font_size: 40.sp(),
                    ..TextStyle::DEFAULT
                },
                modifier = Modifier.background(Color::BLACK).test_tag(TAG),
            );
        },
    );
    multi_ui.wait_for_idle();
    assert_eq!(recompositions.get(), 1);
    let img1 = multi_ui.on_node_with_tag(TAG).capture_to_image();
    // First paragraph is solid WHITE; second paragraph's span has BOTH RED (top half) and BLUE (bottom half)!
    img1.assert_contains_color(Color::WHITE);
    img1.assert_contains_color(Color::RED);
    img1.assert_contains_color(Color::BLUE);

    span_flipped.set(true);
    multi_ui.wait_for_idle();
    assert_eq!(recompositions.get(), 1, "state toggle must only invalidate draw");
    let img2 = multi_ui.on_node_with_tag(TAG).capture_to_image();
    img2.assert_contains_color(Color::WHITE);
    img2.assert_contains_color(Color::GREEN);
    img2.assert_contains_color(Color::YELLOW);
    img2.assert_does_not_contain_color(Color::RED);
    img2.assert_does_not_contain_color(Color::BLUE);

    // 3. Draw-only restyle (`shadow` toggled) on a `BasicText` with `CustomShaderBrush` preserves
    // the dynamic shader rather than falling back to placeholder black.
    let style_flipped = mutable_state_of(false);
    let has_shadow = mutable_state_of(false);
    let has_shadow_state = has_shadow.as_state();
    let style_brush: Brush = ShaderBrush::custom(StateSplitBrush {
        flipped: style_flipped.as_state(),
        calls: Arc::new(AtomicUsize::new(0)),
    })
    .into();
    let mut restyle_ui = compose_test_with(
        IntSize::new(200, 120),
        Density::new(1.0),
        std::boxed::Box::new(SkiaTextMeasurer::new()),
        move || {
            let shadow = has_shadow_state.get().then_some(Shadow {
                color: Color::WHITE,
                offset: Offset::new(2.0, 2.0),
                blur_radius: 1.0,
            });
            BasicText!(
                "HH",
                style = TextStyle {
                    brush: Some(style_brush.clone()),
                    shadow,
                    font_family: Some(basic_measure_font().to_font_family()),
                    font_size: 40.sp(),
                    ..TextStyle::DEFAULT
                },
                modifier = Modifier.background(Color::BLACK).test_tag(TAG),
            );
        },
    );
    restyle_ui.wait_for_idle();
    restyle_ui.on_node_with_tag(TAG).capture_to_image().assert_contains_color(Color::RED);
    has_shadow.set(true);
    restyle_ui.wait_for_idle();
    let after_shadow = restyle_ui.on_node_with_tag(TAG).capture_to_image();
    after_shadow.assert_contains_color(Color::RED);
    after_shadow.assert_contains_color(Color::BLUE);
}

/// `Canvas::save_layer` must snapshot `Shader::Image` bitmaps when the draw op is recorded inside
/// the open layer (before `restore()`), matching `DrawOp::DrawImage`, and drawing a bitmap into
/// itself via `Shader::Image` (both outside and inside `save_layer`) must not deadlock.
#[test]
fn canvas_save_layer_snapshots_image_shader_at_draw_time() {
    install_skia();

    let src = ImageBitmap::new(4, 4);
    Canvas::new(&src).draw_rect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::RED));

    let dst = ImageBitmap::new(4, 4);
    let mut dst_canvas = Canvas::new(&dst);
    dst_canvas.save_layer(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::BLACK));
    let src_brush = Brush::from(Shader::image(&src, TileMode::Clamp, TileMode::Clamp));
    CanvasDrawScope::draw(
        &mut dst_canvas,
        Density::new(1.0),
        LayoutDirection::Ltr,
        Size::new(4.0, 4.0),
        |scope| {
            let paint = scope.brush_paint(&src_brush, 1.0, PaintStyle::Fill);
            scope.draw_rect(Rect::new(0.0, 0.0, 4.0, 4.0), paint);
        },
    );
    // Mutate `src` BEFORE `dst_canvas.restore()`!
    Canvas::new(&src).draw_rect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::BLUE));
    dst_canvas.restore();

    assert_eq!(dst.to_pixel_map().get(2, 2), Color::RED);

    // Self-draw via `Shader::Image` inside `save_layer`: left half RED shifted right by 2 px!
    let self_bmp = ImageBitmap::new(4, 4);
    let mut self_canvas = Canvas::new(&self_bmp);
    self_canvas.draw_rect(Rect::new(0.0, 0.0, 2.0, 4.0), Paint::fill(Color::RED));
    self_canvas.draw_rect(Rect::new(2.0, 0.0, 4.0, 4.0), Paint::fill(Color::GREEN));
    let self_shader = ShaderBrush::new(Shader::image(&self_bmp, TileMode::Clamp, TileMode::Clamp));
    self_shader.set_transform(Some(Matrix::translation(2.0, 0.0)));
    let self_brush = Brush::from(self_shader);
    self_canvas.with_save_layer(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::BLACK), |c| {
        CanvasDrawScope::draw(
            c,
            Density::new(1.0),
            LayoutDirection::Ltr,
            Size::new(4.0, 4.0),
            |scope| {
                let paint = scope.brush_paint(&self_brush, 1.0, PaintStyle::Fill);
                scope.draw_rect(Rect::new(2.0, 0.0, 4.0, 4.0), paint);
            },
        );
    });
    let self_map = self_bmp.to_pixel_map();
    assert_eq!(self_map.get(0, 1), Color::RED);
    assert_eq!(self_map.get(3, 1), Color::RED);
}

#[test]
fn all_bitmap_configs_alpha_modes_and_wide_color_spaces_round_trip() {
    install_skia();

    // Opaque `F16` (`has_alpha = false`) in both `SRGB` and `DISPLAY_P3`, plus `ADOBE_RGB` `Rgb565`
    // and `OKLAB` `Argb8888`!
    let cases: [(ImageBitmapConfig, bool, &ColorSpace); 8] = [
        (ImageBitmapConfig::F16, false, &ColorSpaces::SRGB),
        (ImageBitmapConfig::F16, true, &ColorSpaces::SRGB),
        (ImageBitmapConfig::F16, false, &ColorSpaces::DISPLAY_P3),
        (ImageBitmapConfig::F16, true, &ColorSpaces::DISPLAY_P3),
        (ImageBitmapConfig::Rgb565, false, &ColorSpaces::DISPLAY_P3),
        (ImageBitmapConfig::Argb8888, false, &ColorSpaces::ADOBE_RGB),
        (ImageBitmapConfig::Gpu, true, &ColorSpaces::DISPLAY_P3),
        (ImageBitmapConfig::Argb8888, true, &ColorSpaces::OKLAB),
    ];
    for (config, has_alpha, space) in cases {
        let bmp = ImageBitmap::new_with(4, 4, config, has_alpha, space);
        Canvas::new(&bmp).draw_rect(Rect::new(0.0, 0.0, 2.0, 4.0), Paint::fill(Color::RED));
        let left = bmp.to_pixel_map().get(0, 1);
        assert!(
            left.red() > 0.90 && left.green() < 0.10 && left.blue() < 0.10 && left.alpha() > 0.98,
            "{config:?} has_alpha={has_alpha} space={}: left={left:?}",
            space.name()
        );
        if !has_alpha {
            // Untouched right half of an opaque bitmap is opaque BLACK, not transparent!
            let right = bmp.to_pixel_map().get(3, 1);
            assert_eq!(
                right,
                Color::BLACK,
                "{config:?} has_alpha=false space={}: untouched right={right:?}",
                space.name()
            );
        }

        // Also sample `bmp` through `Shader::image` onto an sRGB target bitmap!
        let out = ImageBitmap::new(4, 4);
        Canvas::new(&out).draw_rect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::BLUE));
        let img_brush = Brush::from(Shader::image(&bmp, TileMode::Clamp, TileMode::Clamp));
        let mut out_canvas = Canvas::new(&out);
        CanvasDrawScope::draw(
            &mut out_canvas,
            Density::new(1.0),
            LayoutDirection::Ltr,
            Size::new(4.0, 4.0),
            |scope| {
                let paint = scope.brush_paint(&img_brush, 1.0, PaintStyle::Fill);
                scope.draw_rect(Rect::new(0.0, 0.0, 4.0, 4.0), paint);
            },
        );
        let sampled_left = out.to_pixel_map().get(0, 1);
        assert!(
            sampled_left.red() > 0.90 && sampled_left.blue() < 0.10,
            "Shader::image {config:?} has_alpha={has_alpha} space={}: got {sampled_left:?}",
            space.name()
        );
        if !has_alpha {
            let sampled_right = out.to_pixel_map().get(3, 1);
            assert_eq!(
                sampled_right,
                Color::BLACK,
                "Shader::image untouched opaque pixel must cover BLUE background: {config:?} space={}",
                space.name()
            );
        }
    }
}

#[test]
fn modulate_alpha_and_single_drawing_fade_match_offscreen_for_composite_shader() {
    install_skia();

    let render = |strategy: CompositingStrategy, opts: Optimizations| {
        let mut host = compose::ui::platform::UiHost::headless(40, 40);
        host.set_content(move || {
            let brush = Brush::composite(
                Brush::solid(Color::RED),
                Brush::horizontal_gradient([Color::WHITE, Color::WHITE]),
                BlendMode::Modulate,
            )
            .with_alpha(0.8);
            compose::foundation::layout::Spacer(
                Modifier
                    .size(40.dp())
                    .graphics_layer_with(GraphicsLayerScope {
                        alpha: 0.5,
                        compositing_strategy: strategy,
                        ..GraphicsLayerScope::default()
                    })
                    .background(brush),
            );
        });
        host.frame();
        let mut renderer = RasterRenderer::new(40, 40);
        renderer.set_optimizations(opts);
        host.with_tree(|tree| {
            renderer.render(tree.display_list(), Color::WHITE, 1.0, &FixedTextMeasurer::default());
        });
        renderer.pixel(20, 20)
    };

    let offscreen = render(CompositingStrategy::Offscreen, Optimizations::NONE);
    let modulated = render(CompositingStrategy::ModulateAlpha, Optimizations::NONE);
    let single_fade = render(
        CompositingStrategy::Auto,
        Optimizations::NONE.with(Optimization::SINGLE_DRAWING_FADE, true),
    );
    for (label, got) in [("ModulateAlpha", modulated), ("SINGLE_DRAWING_FADE", single_fade)] {
        assert!((got.red() - offscreen.red()).abs() <= 0.02, "{label}: {got:?} vs {offscreen:?}");
        assert!(
            (got.green() - offscreen.green()).abs() <= 0.02,
            "{label}: {got:?} vs {offscreen:?}"
        );
        assert!((got.blue() - offscreen.blue()).abs() <= 0.02, "{label}: {got:?} vs {offscreen:?}");
    }
}

#[test]
fn filter_quality_updates_dynamically_on_image_and_bitmap_icon_tints() {
    install_skia();

    let sample = ImageBitmap::new(2, 2);
    {
        let mut c = Canvas::new(&sample);
        c.draw_rect(Rect::new(0.0, 0.0, 2.0, 2.0), Paint::fill(Color::WHITE));
        c.draw_rect(Rect::new(0.0, 0.0, 1.0, 2.0), Paint::fill(Color::RED));
    }

    let quality = mutable_state_of(FilterQuality::None);
    let quality_state = quality.as_state();
    let sample_for_img = sample.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(40, 40), move || {
        Image!(
            sample_for_img.clone(),
            None,
            modifier = Modifier.size(40.dp()).test_tag(TAG),
            filter_quality = quality_state.get(),
        );
    });
    let nearest = ui.on_node_with_tag(TAG).capture_to_image();
    // Sharp boundary at x = 20: x = 19 is pure RED, x = 20 is pure WHITE.
    assert_eq!(nearest.pixel(19, 20), Color::RED);
    assert_eq!(nearest.pixel(20, 20), Color::WHITE);

    quality.set(FilterQuality::Low);
    ui.wait_for_idle();
    let bilinear = ui.on_node_with_tag(TAG).capture_to_image();
    assert_ne!(
        bilinear.pixel(19, 20),
        Color::RED,
        "FilterQuality::Low interpolates across x=19..20"
    );

    // `BitmapIcon` tints with `SrcIn` and uses `FilterQuality::Low`.
    let mut icon_ui = ComposeTestRule::sized(IntSize::new(40, 40), move || {
        BitmapIcon!(
            sample.clone(),
            Some("icon"),
            modifier = Modifier.size(40.dp()).background(Color::BLACK).test_tag(TAG),
            tint = Color::GREEN,
        );
    });
    icon_ui.on_node_with_tag(TAG).capture_to_image().assert_pixel_color(Color::GREEN, 20, 20);
}

/// A recorded `GraphicsLayer` object drawn with `ModulateAlpha` fades the shader-brush fills it
/// replays (`PaintShader::scale_alpha` in `replay_layer`), as it fades their colors: a red
/// shader brush at layer alpha 0.5 over white is half red, not opaque red.
#[test]
fn a_modulate_alpha_graphics_layer_object_fades_its_shader_brush_fills() {
    use compose::foundation::layout::Box;
    use compose::ui::remember_graphics_layer;
    set_capture_renderer(compose_skia::raster_colors);
    let mut ui = ComposeTestRule::sized(IntSize::new(40, 40), move || {
        let layer = remember_graphics_layer();
        Box!(
            modifier = Modifier.test_tag(TAG).size(40.dp()).background(Color::WHITE).draw_behind(
                move |scope| {
                    let red = Brush::from(Shader::linear_gradient(
                        Offset::ZERO,
                        Offset::new(40.0, 0.0),
                        [Color::RED, Color::RED],
                        TileMode::Clamp,
                    ));
                    let bounds = scope.bounds();
                    layer.record(scope, IntSize::new(40, 40), move |scope| {
                        let paint = scope.brush_paint(&red, 1.0, PaintStyle::Fill);
                        scope.draw_rect(bounds, paint);
                    });
                    layer.set_alpha(0.5);
                    layer.set_compositing_strategy(CompositingStrategy::ModulateAlpha);
                    scope.draw_layer(&layer);
                }
            )
        );
    });
    let pixel = ui.on_node_with_tag(TAG).capture_to_image().pixel(20, 20);
    let expected = Color::RED.with_alpha(0.5).composite_over(Color::WHITE);
    let green = |c: Color| ((c.to_argb() >> 8) & 0xff) as u8;
    assert!(green(pixel).abs_diff(green(expected)) <= 3, "{pixel:?} vs {expected:?}");
}
