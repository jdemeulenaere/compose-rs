//! Painters and `Modifier.paint` drawn by Skia, as Compose's `PainterModifierTest` and
//! `PainterTest` check them on a device.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose::foundation::layout::*;
use compose::foundation::{LazyColumn, PainterImage};
use compose::material3::{PainterIcon, PainterIconWithTintProducer};
use compose::runtime::*;
use compose::test::*;
use compose::ui::{
    Alignment, BitmapPainter, BlendMode, Color, ColorFilter, ColorPainter, ColorProducer,
    ContentScale, CustomPainter, DrawScope, ImageBitmap, IntSize, IntrinsicSize,
    LOCAL_LAYOUT_DIRECTION, Layout, LayoutDirection, Modifier, Paint, Painter, Size, UnitExt,
    measure_policy,
};

const TAG: &str = "painted";

/// A rule capturing with Skia, at density 1 in a 500 px root.
fn painted(content: impl Fn() + 'static) -> ComposeTestRule {
    set_capture_renderer(compose_skia::raster_colors);
    ComposeTestRule::sized(IntSize::new(500, 500), content)
}

/// Fills what it's drawn into with its color: red, blue right to left.
struct TestPainter {
    size: Size,
    color: Cell<Color>,
}

fn test_painter(width: f32, height: f32) -> Painter {
    Painter::new(TestPainter { size: Size::new(width, height), color: Cell::new(Color::RED) })
}

impl CustomPainter for TestPainter {
    fn intrinsic_size(&self) -> Size {
        self.size
    }
    fn on_draw(&self, scope: &mut DrawScope<'_>) {
        let bounds = scope.bounds();
        scope.draw_rect(bounds, Paint::fill(self.color.get()));
    }
    fn apply_layout_direction(&self, layout_direction: LayoutDirection) -> bool {
        let rtl = layout_direction == LayoutDirection::Rtl;
        self.color.set(if rtl { Color::BLUE } else { Color::RED });
        true
    }
}

/// A 100 px node over white painting a 100 px test painter, as the test's `TestPainter()`.
#[composable]
fn PaintedSquare(alpha: f32, color_filter: Option<ColorFilter>, direction: LayoutDirection) {
    let painter = remember(|| test_painter(100.0, 100.0));
    CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), move || {
        Box!(
            modifier = Modifier.test_tag(TAG).background(Color::WHITE).size(100.dp()).paint_with(
                painter.clone(),
                true,
                Alignment::CENTER,
                ContentScale::Inside,
                alpha,
                color_filter
            )
        );
    });
}

/// A box of `modifier` measured with its minimums at 0, as in the test's `NoMinSizeContainer`.
#[composable]
fn NoMinSize(modifier: Modifier) {
    Layout!(
        modifier = Modifier,
        measure_policy = measure_policy(|scope, measurables, constraints| {
            let loose = constraints.loosen();
            let (mut width, mut height) = (0, 0);
            let placeables: Vec<_> =
                measurables.iter().map(|measurable| scope.measure(*measurable, loose)).collect();
            for placeable in &placeables {
                width = width.max(placeable.width);
                height = height.max(placeable.height);
                scope.place(placeable, 0, 0);
            }
            scope.layout(width, height)
        }),
        {
            Box!(modifier = modifier.clone());
        }
    );
}

fn capture(ui: &mut ComposeTestRule) -> CapturedImage {
    ui.on_node_with_tag(TAG).capture_to_image()
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/PainterModifierTest.kt, testPainterModifierColorFilter
#[test]
fn a_painters_color_filter_tints_it() {
    let filter = ColorFilter::tint_with(Color::CYAN, BlendMode::SrcIn);
    let mut ui = painted(move || PaintedSquare(1.0, Some(filter), LayoutDirection::Ltr));
    capture(&mut ui).assert_pixel_color(Color::CYAN, 50, 50);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/PainterModifierTest.kt, testPainterModifierAlpha
// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/painter/PainterTest.kt, testPainterAlpha
#[test]
fn a_painters_alpha_fades_it_over_what_is_under_it() {
    let mut ui = painted(|| PaintedSquare(0.5, None, LayoutDirection::Ltr));
    let pixel = capture(&mut ui).pixel(50, 50);
    let expected = Color::RED.with_alpha(0.5).composite_over(Color::WHITE);
    for (channel, wanted) in [
        (((pixel.to_argb() >> 16) as u8), ((expected.to_argb() >> 16) as u8)),
        (((pixel.to_argb() >> 8) as u8), ((expected.to_argb() >> 8) as u8)),
        ((pixel.to_argb() as u8), (expected.to_argb() as u8)),
    ] {
        assert!(channel.abs_diff(wanted) <= 2, "{pixel:?} against {expected:?}");
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/PainterModifierTest.kt, testPainterModifierRtl
// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/painter/PainterTest.kt, testPainterRtl
#[test]
fn a_painter_learns_its_layout_direction() {
    let mut ui = painted(|| PaintedSquare(1.0, None, LayoutDirection::Rtl));
    capture(&mut ui).assert_pixel_color(Color::BLUE, 50, 50);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/PainterModifierTest.kt, testPainterAspectRatioMaintainedInSmallerParent
#[test]
fn a_larger_painter_is_scaled_down_inside_its_node() {
    let mut ui = painted(|| {
        Box!(modifier = Modifier.test_tag(TAG).size(300.dp()).background(Color::WHITE), {
            Box!(
                modifier = Modifier.padding(100.dp()).size(100.dp()).paint_with(
                    test_painter(200.0, 200.0),
                    true,
                    Alignment::CENTER,
                    ContentScale::Inside,
                    1.0,
                    None::<ColorFilter>
                )
            );
        });
    });
    let image = capture(&mut ui);
    image.assert_pixel_color(Color::WHITE, 99, 99);
    image.assert_pixel_color(Color::RED, 101, 101);
    image.assert_pixel_color(Color::RED, 199, 199);
    image.assert_pixel_color(Color::WHITE, 201, 201);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/PainterModifierTest.kt, testPainterAlignedBottomRightIfSmallerThanParent
#[test]
fn a_smaller_painter_is_aligned_in_its_node() {
    let mut ui = painted(|| {
        Box!(
            modifier = Modifier.test_tag(TAG).size(200.dp()).background(Color::WHITE).paint_with(
                test_painter(100.0, 100.0),
                true,
                Alignment::BOTTOM_END,
                ContentScale::Inside,
                1.0,
                None::<ColorFilter>
            )
        );
    });
    let image = capture(&mut ui);
    for (x, y) in [(199, 199), (100, 199), (100, 101), (199, 101)] {
        image.assert_pixel_color(Color::RED, x, y);
    }
    for (x, y) in [(99, 199), (99, 99), (199, 99)] {
        image.assert_pixel_color(Color::WHITE, x, y);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/PainterModifierTest.kt, testPainterModifierIntrinsicSize
#[test]
fn a_node_takes_its_painters_intrinsic_size() {
    let mut ui = painted(|| NoMinSize(Modifier.test_tag(TAG).paint(test_painter(100.0, 100.0))));
    let image = capture(&mut ui);
    assert_eq!(image.size(), IntSize::new(100, 100));
    image.assert_pixels(None, |_| Some(Color::RED));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/PainterModifierTest.kt, testPainterIntrinsicSizeDoesNotExceedMax, testPainterNotSizedToIntrinsics
#[test]
fn a_fixed_node_scales_its_painter_into_it() {
    for size_to_intrinsics in [true, false] {
        let mut ui = painted(move || {
            Box!(modifier = Modifier.size(100.dp()).background(Color::WHITE), {
                Box!(
                    modifier = Modifier.test_tag(TAG).size(50.dp()).paint_with(
                        test_painter(100.0, 100.0),
                        size_to_intrinsics,
                        Alignment::TOP_START,
                        ContentScale::Inside,
                        1.0,
                        None::<ColorFilter>
                    )
                );
            });
        });
        let image = capture(&mut ui);
        assert_eq!(image.size(), IntSize::new(50, 50));
        image.assert_pixels(None, |_| Some(Color::RED));
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/PainterModifierTest.kt, testUnboundedPainterDoesNotCrash
#[test]
fn a_color_painter_in_an_unbounded_column_does_not_crash() {
    let mut ui = painted(|| {
        LazyColumn!(
            modifier = Modifier.fill_max_size().padding(16.dp()),
            |scope: compose::foundation::LazyListScope| {
                scope.item(|| {
                    PainterImage!(Painter::new(ColorPainter::new(Color::BLACK)), Some(""));
                });
            }
        );
    });
    ui.wait_for_idle();
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/PainterModifierTest.kt, testPainterIntrinsicWidthRespected, testPainterIntrinsicHeightRespected
#[test]
fn a_painters_intrinsic_size_is_the_nodes_intrinsic_size() {
    for width in [true, false] {
        let mut ui = painted(move || {
            let modifier = if width {
                Modifier.width(IntrinsicSize::Min)
            } else {
                Modifier.height(IntrinsicSize::Min)
            };
            NoMinSize(modifier.test_tag(TAG).paint(test_painter(100.0, 100.0)));
        });
        let image = capture(&mut ui);
        assert_eq!(image.size(), IntSize::new(100, 100));
        image.assert_pixels(None, |_| Some(Color::RED));
    }
}

/// The size of a node painting a painter of `painter_size` that draws nothing, scaled by
/// `content_scale`, under `modifier`.
fn scaled_node_size(
    modifier: Modifier,
    content_scale: ContentScale,
    painter_size: Size,
) -> IntSize {
    struct Empty(Size);
    impl CustomPainter for Empty {
        fn intrinsic_size(&self) -> Size {
            self.0
        }
        fn on_draw(&self, _scope: &mut DrawScope<'_>) {}
    }
    let painter = Painter::new(Empty(painter_size));
    let mut ui = painted(move || {
        let (modifier, painter) = (modifier.clone(), painter.clone());
        Box!(modifier = Modifier.wrap_content_size(), {
            let (modifier, painter) = (modifier.clone(), painter.clone());
            Box!(
                modifier = modifier.test_tag(TAG).paint_with(
                    painter,
                    true,
                    Alignment::CENTER,
                    content_scale,
                    1.0,
                    None::<ColorFilter>
                )
            );
        });
    });
    let bounds = ui.on_node_with_tag(TAG).fetch_semantics_snapshot().bounds;
    IntSize::new(bounds.width() as i32, bounds.height() as i32)
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/PainterModifierTest.kt, testPainterFixedHeightScalesDownWidth, testPainterFixedWidthScalesDownHeight, testPainterFixedDimensionUnchanged
#[test]
fn a_fixed_side_scales_the_other_side_down() {
    let painter = Size::new(400.0, 200.0);
    let fixed_height = Modifier.required_height(100.dp()).width_in(0.dp(), 300.dp());
    assert_eq!(
        scaled_node_size(fixed_height, ContentScale::Inside, painter),
        IntSize::new(200, 100)
    );
    let fixed_width = Modifier.required_width(100.dp()).height_in(0.dp(), 300.dp());
    assert_eq!(scaled_node_size(fixed_width, ContentScale::Inside, painter), IntSize::new(100, 50));
    let both = Modifier.required_width(250.dp()).required_height(400.dp());
    assert_eq!(
        scaled_node_size(both, ContentScale::Fit, Size::new(1000.0, 375.0)),
        IntSize::new(250, 400)
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/PainterModifierTest.kt, testPainterComposableHeightScaledUpWithFixedWidth, testPainterWidthScaledDownWithSmallerHeight
#[test]
fn a_fixed_width_scales_the_height_by_the_content_scale() {
    let painter = Size::new(100.0, 200.0);
    let width = || Modifier.required_width(200.dp()).wrap_content_height();
    assert_eq!(scaled_node_size(width(), ContentScale::FillWidth, painter), IntSize::new(200, 400));
    assert_eq!(scaled_node_size(width(), ContentScale::Inside, painter), IntSize::new(200, 200));
}

/// A 2 by 4 bitmap: its left column red, its right one... red too, as the test's.
fn red_bitmap(width: u32, height: u32) -> ImageBitmap {
    let pixels = [255, 0, 0, 255].repeat((width * height) as usize);
    ImageBitmap::from_rgba_premultiplied(width, height, pixels)
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/PainterModifierTest.kt, testBitmapPainterScalesContent
#[test]
fn a_bitmap_painter_scales_its_bitmap_to_fill_the_height() {
    let image = red_bitmap(50, 100);
    let mut ui = painted(move || {
        Box!(
            modifier = Modifier
                .test_tag(TAG)
                .background(Color::GRAY)
                .required_width(300.dp())
                .required_height(200.dp())
                .paint_with(
                    BitmapPainter::new(image.clone()),
                    true,
                    Alignment::CENTER,
                    ContentScale::FillHeight,
                    1.0,
                    None::<ColorFilter>
                )
        );
    });
    let captured = capture(&mut ui);
    assert_eq!(captured.size(), IntSize::new(300, 200));
    captured.assert_pixel_color(Color::GRAY, 150 - 50 - 5, 0);
    captured.assert_pixel_color(Color::GRAY, 150 + 50 + 5, 199);
    captured.assert_pixel_color(Color::RED, 150 - 50 + 5, 0);
    captured.assert_pixel_color(Color::RED, 150 + 50 - 5, 199);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/PainterModifierTest.kt, testBitmapPainterScalesNonUniformly
#[test]
fn a_bitmap_painter_stretches_to_fill_the_bounds() {
    let image = red_bitmap(10, 20);
    let mut ui = painted(move || {
        Box!(
            modifier = Modifier
                .test_tag(TAG)
                .background(Color::GRAY)
                .required_width(60.dp())
                .required_height(40.dp())
                .paint_with(
                    BitmapPainter::new(image.clone()),
                    true,
                    Alignment::CENTER,
                    ContentScale::FillBounds,
                    1.0,
                    None::<ColorFilter>
                )
        );
    });
    capture(&mut ui).assert_pixels(Some(IntSize::new(60, 40)), |_| Some(Color::RED));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/PainterModifierTest.kt, testInspectable
#[test]
fn the_modifier_is_named_paint() {
    let painter = test_painter(10.0, 20.0);
    let modifier = Modifier.paint(painter.clone());
    assert!(format!("{modifier:?}").contains("paint"), "{modifier:?}");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/PainterModifierTest.kt, testBackgroundPainterChain
#[test]
fn a_second_painter_draws_over_the_first() {
    let first = test_painter(100.0, 100.0);
    let second = TestPainter { size: Size::new(100.0, 100.0), color: Cell::new(Color::RED) };
    second.color.set(Color::BLUE.with_alpha(0.5));
    // Its color is set before drawing; a left-to-right drawing doesn't reset it.
    struct Fixed(TestPainter);
    impl CustomPainter for Fixed {
        fn intrinsic_size(&self) -> Size {
            self.0.size
        }
        fn on_draw(&self, scope: &mut DrawScope<'_>) {
            self.0.on_draw(scope);
        }
    }
    let second = Painter::new(Fixed(second));
    let mut ui = painted(move || {
        Box!(
            modifier =
                Modifier.test_tag(TAG).size(2.dp()).paint(first.clone()).paint(second.clone())
        );
    });
    let expected = Color::BLUE.with_alpha(0.5).composite_over(Color::RED);
    capture(&mut ui).assert_pixels(Some(IntSize::new(2, 2)), |_| Some(expected));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/PainterModifierTest.kt, testUpdatingPainterWithTheSameIntrinsicsSize, testUpdatingPainterWithTheDifferentIntrinsicsSize
#[test]
fn a_new_painter_draws_and_sizes_the_node() {
    for size in [10.0, 5.0] {
        let painter = mutable_state_of(test_painter(10.0, 10.0));
        let handle = painter.handle();
        let mut ui = painted(move || {
            Box!(modifier = Modifier.test_tag(TAG).paint(handle.get()));
        });
        let blue = TestPainter { size: Size::new(size, size), color: Cell::new(Color::BLUE) };
        struct Blue(TestPainter);
        impl CustomPainter for Blue {
            fn intrinsic_size(&self) -> Size {
                self.0.size
            }
            fn on_draw(&self, scope: &mut DrawScope<'_>) {
                self.0.on_draw(scope);
            }
        }
        painter.set(Painter::new(Blue(blue)));
        ui.wait_for_idle();
        let side = size as i32;
        capture(&mut ui).assert_pixels(Some(IntSize::new(side, side)), |_| Some(Color::BLUE));
    }
}

/// Counts its drawings and the alphas it's given.
struct Counting {
    draws: Rc<Cell<usize>>,
    color: Cell<Color>,
}

impl CustomPainter for Counting {
    fn intrinsic_size(&self) -> Size {
        Size::new(100.0, 100.0)
    }
    fn on_draw(&self, scope: &mut DrawScope<'_>) {
        self.draws.set(self.draws.get() + 1);
        let bounds = scope.bounds();
        scope.draw_rect(bounds, Paint::fill(self.color.get()));
    }
    fn apply_alpha(&self, alpha: f32) -> bool {
        self.color.set(Color::RED.with_alpha(alpha));
        true
    }
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/painter/PainterTest.kt, testPainterDidDraw, testPainterCustomAlpha
#[test]
fn a_painter_draws_and_takes_its_alpha() {
    let draws = Rc::new(Cell::new(0));
    let painter = Painter::new(Counting { draws: draws.clone(), color: Cell::new(Color::RED) });
    assert_eq!(painter.intrinsic_size(), Size::new(100.0, 100.0));
    assert_eq!(draws.get(), 0);
    let mut ui = painted(move || {
        Box!(
            modifier = Modifier.test_tag(TAG).background(Color::WHITE).paint_with(
                painter.clone(),
                true,
                Alignment::CENTER,
                ContentScale::Inside,
                0.5,
                None::<ColorFilter>
            )
        );
    });
    assert!(draws.get() > 0);
    let pixel = capture(&mut ui).pixel(50, 50);
    let expected = Color::RED.with_alpha(0.5).composite_over(Color::WHITE);
    assert!(
        ((pixel.to_argb() >> 8) as u8).abs_diff((expected.to_argb() >> 8) as u8) <= 2,
        "{pixel:?} against {expected:?}"
    );
    // It took the alpha: no layer.
    let dump = ui.host().display_list().dump();
    assert!(!dump.contains("save layer"), "{dump}");
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/painter/PainterTest.kt, testColorFilter
#[test]
fn a_color_painter_takes_its_filter() {
    let mut ui = painted(|| {
        Box!(
            modifier = Modifier.test_tag(TAG).size(100.dp()).paint_with(
                ColorPainter::new(Color::BLACK),
                true,
                Alignment::CENTER,
                ContentScale::Inside,
                1.0,
                ColorFilter::tint(Color::BLUE)
            )
        );
    });
    capture(&mut ui).assert_pixel_color(Color::BLUE, 50, 50);
    let dump = ui.host().display_list().dump();
    assert!(!dump.contains("save layer"), "{dump}");
}

/// A bitmap painter is equal to another of the same bitmap and part, a color painter to one of
/// the same color, a custom painter to itself only.
#[test]
fn painters_compare_as_composes() {
    let image = red_bitmap(2, 2);
    assert_eq!(
        Painter::new(BitmapPainter::new(image.clone())),
        Painter::new(BitmapPainter::new(image.clone()))
    );
    assert_ne!(
        Painter::new(BitmapPainter::new(image.clone())),
        Painter::new(BitmapPainter::new(red_bitmap(2, 2)))
    );
    assert_eq!(Painter::new(ColorPainter::new(Color::RED)), ColorPainter::new(Color::RED).into());
    let custom = test_painter(1.0, 1.0);
    assert_eq!(custom, custom.clone());
    assert_ne!(custom, test_painter(1.0, 1.0));
    let outside = std::panic::catch_unwind(|| {
        BitmapPainter::with_source(
            red_bitmap(2, 2),
            compose::ui::IntOffset::new(0, 0),
            IntSize::new(3, 1),
        )
    });
    assert!(outside.is_err(), "a source larger than the image");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconTest.kt, painter_noIntrinsicSize_dimensions, painter_withIntrinsicSize_dimensions
#[test]
fn a_painter_icon_is_its_painters_size_or_24_dp() {
    for (painter, size) in [
        (Painter::new(ColorPainter::new(Color::RED)), IntSize::new(24, 24)),
        (Painter::new(BitmapPainter::new(red_bitmap(35, 83))), IntSize::new(35, 83)),
    ] {
        let mut ui = painted(move || {
            PainterIcon!(painter.clone(), None::<&str>, modifier = Modifier.test_tag(TAG));
        });
        let bounds = ui.on_node_with_tag(TAG).fetch_semantics_snapshot().bounds;
        assert_eq!((bounds.width() as i32, bounds.height() as i32), (size.width, size.height));
    }
}

/// `PainterIconWithTintProducer` tints the icon with what its producer gives when it draws: a
/// state it reads draws it again.
#[test]
fn an_icons_tint_producer_tints_it_when_drawn() {
    let tint = mutable_state_of(Color::RED);
    let handle = tint.handle();
    let compositions = Rc::new(Cell::new(0));
    let counted = compositions.clone();
    let mut ui = painted(move || {
        counted.set(counted.get() + 1);
        let producer = remember(|| ColorProducer(Rc::new(move || handle.get())));
        PainterIconWithTintProducer!(
            Painter::new(ColorPainter::new(Color::BLACK)),
            Some(producer),
            None::<&str>,
            modifier = Modifier.test_tag(TAG)
        );
    });
    let image = capture(&mut ui);
    assert_eq!(image.size(), IntSize::new(24, 24));
    image.assert_pixel_color(Color::RED, 12, 12);
    tint.set(Color::GREEN);
    ui.wait_for_idle();
    capture(&mut ui).assert_pixel_color(Color::GREEN, 12, 12);
    assert_eq!(compositions.get(), 1, "the tint is read when drawing, not composing");
}

/// A new alpha, or a new painter of the same intrinsic size, draws the node again without
/// measuring it; a new intrinsic size measures it, as Compose's `PainterNode` invalidates itself.
#[test]
fn a_painter_node_is_measured_again_only_for_new_intrinsics() {
    let alpha = mutable_state_of(1.0f32);
    let side = mutable_state_of(10.0f32);
    let (alpha_handle, side_handle) = (alpha.handle(), side.handle());
    let measures = Rc::new(Cell::new(0));
    let counted = measures.clone();
    let mut ui = painted(move || {
        let counted = counted.clone();
        let counting = remember(move || {
            Modifier.layout(move |scope, measurable, constraints| {
                counted.set(counted.get() + 1);
                let placeable = scope.measure(measurable, constraints);
                scope.place(&placeable, 0, 0);
                scope.layout(placeable.width, placeable.height)
            })
        });
        let side = side_handle.get();
        Box!(
            modifier = counting.test_tag(TAG).paint_with(
                test_painter(side, side),
                true,
                Alignment::CENTER,
                ContentScale::Inside,
                alpha_handle.get(),
                None::<ColorFilter>
            )
        );
    });
    let first = measures.get();
    alpha.set(0.5);
    ui.wait_for_idle();
    assert_eq!(measures.get(), first, "a new alpha and painter of the same size");
    let faded = capture(&mut ui).pixel(5, 5);
    assert!(((faded.to_argb() >> 24) as u8) < 255 || faded != Color::RED, "drawn again: {faded:?}");
    side.set(20.0);
    ui.wait_for_idle();
    assert!(measures.get() > first, "a new intrinsic size");
    assert_eq!(capture(&mut ui).size(), IntSize::new(20, 20));
}

/// 100 px of blue with a 50 px red square in its middle, as the test's source image.
fn blue_with_red_middle() -> ImageBitmap {
    let mut pixels = Vec::with_capacity(100 * 100 * 4);
    for y in 0..100 {
        for x in 0..100 {
            let red = (25..75).contains(&x) && (25..75).contains(&y);
            pixels.extend_from_slice(if red { &[255, 0, 0, 255] } else { &[0, 0, 255, 255] });
        }
    }
    ImageBitmap::from_rgba_premultiplied(100, 100, pixels)
}

/// A node at the top left of a 200 px white root painting `painter` at its intrinsic size.
fn painter_on_white(painter: Painter, alpha: f32, filter: Option<ColorFilter>) -> CapturedImage {
    let mut ui = painted(move || {
        let painter = painter.clone();
        Box!(modifier = Modifier.test_tag(TAG).size(200.dp()).background(Color::WHITE), {
            Box!(
                modifier = Modifier.paint_with(
                    painter.clone(),
                    true,
                    Alignment::TOP_START,
                    ContentScale::None,
                    alpha,
                    filter
                )
            );
        });
    });
    let image = capture(&mut ui);
    let dump = ui.host().display_list().dump();
    assert!(!dump.contains("save layer"), "a bitmap painter fades and filters itself: {dump}");
    image
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/painter/BitmapPainterTest.kt, testBitmapPainter, testBitmapPainterAppliedAlpha, testBitmapPainterTint
#[test]
fn a_bitmap_painter_draws_fades_and_tints_its_bitmap() {
    let painter = || Painter::new(BitmapPainter::new(blue_with_red_middle()));
    let image = painter_on_white(painter(), 1.0, None);
    for (x, y) in [(195, 5), (195, 195), (5, 195)] {
        image.assert_pixel_color(Color::WHITE, x, y);
    }
    image.assert_pixel_color(Color::RED, 30, 70);
    let faded = painter_on_white(painter(), 0.5, None).pixel(50, 50);
    let expected = Color::RED.with_alpha(0.5).composite_over(Color::WHITE);
    assert!(
        ((faded.to_argb() >> 8) as u8).abs_diff((expected.to_argb() >> 8) as u8) <= 2,
        "{faded:?} against {expected:?}"
    );
    let tinted = painter_on_white(painter(), 1.0, Some(ColorFilter::tint(Color::CYAN)));
    tinted.assert_pixel_color(Color::WHITE, 195, 5);
    tinted.assert_pixel_color(Color::CYAN, 30, 70);
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/painter/BitmapPainterTest.kt, testImageSrcBounds, testInvalidLeftBoundThrows, testInvalidTopBoundThrows, testInvalidRightBoundThrows, testInvalidBottomBoundThrows, testRightLessThanLeftThrows, testTopLessThanBottomThrows
#[test]
fn a_bitmap_painter_draws_the_part_of_its_bitmap_asked_for() {
    use compose::ui::IntOffset;
    let quarter = |x, y| {
        BitmapPainter::with_source(
            blue_with_red_middle(),
            IntOffset::new(x, y),
            IntSize::new(50, 50),
        )
    };
    assert_eq!(quarter(0, 0).intrinsic_size(), Size::new(50.0, 50.0));
    // Each quarter is blue but at the image's middle, red.
    for (x, y, red_corner) in
        [(0, 0, (49, 49)), (50, 0, (0, 49)), (0, 50, (49, 0)), (50, 50, (0, 0))]
    {
        let image = painter_on_white(Painter::new(quarter(x, y)), 1.0, None);
        for corner in [(0, 0), (49, 0), (0, 49), (49, 49)] {
            let expected = if corner == red_corner { Color::RED } else { Color::BLUE };
            image.assert_pixel_color(expected, corner.0, corner.1);
        }
    }
    for (offset, size) in [
        (IntOffset::new(-1, 1), IntSize::new(10, 10)),
        (IntOffset::new(0, -1), IntSize::new(10, 10)),
        (IntOffset::new(0, 0), IntSize::new(101, 10)),
        (IntOffset::new(0, 0), IntSize::new(10, 101)),
        (IntOffset::new(50, 0), IntSize::new(-40, 10)),
        (IntOffset::new(0, 100), IntSize::new(-90, -90)),
    ] {
        let made = std::panic::catch_unwind(|| {
            BitmapPainter::with_source(blue_with_red_middle(), offset, size)
        });
        assert!(made.is_err(), "{offset:?} {size:?}");
    }
}

fn diagonal_sample_bitmap() -> ImageBitmap {
    compose::ui::platform::set_bitmap_rasterizer(std::sync::Arc::new(
        compose_skia::SkiaBitmapRasterizer,
    ));
    let sample = ImageBitmap::new(3, 3);
    let mut canvas = compose::ui::canvas::Canvas::new(&sample);
    canvas.draw_rect(compose::ui::Rect::new(0.0, 0.0, 3.0, 3.0), Paint::fill(Color::WHITE));
    canvas.draw_rect(compose::ui::Rect::new(0.0, 0.0, 1.0, 1.0), Paint::fill(Color::RED));
    canvas.draw_rect(compose::ui::Rect::new(1.0, 1.0, 2.0, 2.0), Paint::fill(Color::BLUE));
    canvas.draw_rect(compose::ui::Rect::new(2.0, 2.0, 3.0, 3.0), Paint::fill(Color::GREEN));
    sample
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/painter/BitmapPainterTest.kt, testFilterQualityNone
#[test]
fn bitmap_painter_filter_quality_none_samples_nearest() {
    use compose::ui::canvas::{Canvas, CanvasDrawScope};
    use compose::ui::{Density, FilterQuality, LayoutDirection, Rect};

    let sample = diagonal_sample_bitmap();
    let dst = ImageBitmap::new(90, 90);
    let mut canvas = Canvas::new(&dst);
    let painter = Painter::new(BitmapPainter::with_filter_quality(
        sample,
        compose::ui::IntOffset::ZERO,
        IntSize::new(3, 3),
        FilterQuality::None,
    ));
    CanvasDrawScope::draw(
        &mut canvas,
        Density::new(1.0),
        LayoutDirection::Ltr,
        Size::new(90.0, 90.0),
        |scope| {
            scope.draw_rect(Rect::new(0.0, 0.0, 90.0, 90.0), Paint::fill(Color::WHITE));
            painter.draw(scope, Size::new(90.0, 90.0), 1.0, None);
        },
    );
    let map = dst.to_pixel_map();
    for i in 0..30 {
        for j in 0..30 {
            assert_eq!(map.get(i, j), Color::RED, "at {i}, {j}");
        }
    }
    for i in 30..60 {
        for j in 30..60 {
            assert_eq!(map.get(i, j), Color::BLUE, "at {i}, {j}");
        }
    }
    for i in 60..90 {
        for j in 60..90 {
            assert_eq!(map.get(i, j), Color::GREEN, "at {i}, {j}");
        }
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ImageTest.kt, testImageFilterQualityNone
#[test]
fn image_composable_filter_quality_none_and_color_filter() {
    use compose::foundation::Image;
    use compose::ui::FilterQuality;

    compose::ui::platform::set_bitmap_rasterizer(std::sync::Arc::new(
        compose_skia::SkiaBitmapRasterizer,
    ));
    let sample = ImageBitmap::new(2, 2);
    {
        let mut canvas = compose::ui::canvas::Canvas::new(&sample);
        canvas.draw_rect(compose::ui::Rect::new(0.0, 0.0, 2.0, 2.0), Paint::fill(Color::WHITE));
        canvas.draw_rect(compose::ui::Rect::new(0.0, 0.0, 1.0, 1.0), Paint::fill(Color::RED));
        canvas.draw_rect(compose::ui::Rect::new(1.0, 1.0, 2.0, 2.0), Paint::fill(Color::BLUE));
    }
    set_capture_renderer(compose_skia::raster_colors);
    let sample_for_none = sample.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(20, 20), move || {
        Image!(
            sample_for_none.clone(),
            Some("FilterQuality None"),
            modifier = Modifier.size(20.dp()).test_tag(TAG),
            filter_quality = FilterQuality::None,
        );
    });
    let captured = ui.on_node_with_tag(TAG).capture_to_image();
    for i in 0..10 {
        for j in 0..10 {
            assert_eq!(captured.pixel(i, j), Color::RED, "at {i}, {j}");
        }
    }
    for i in 11..20 {
        for j in 11..20 {
            assert_eq!(captured.pixel(i, j), Color::BLUE, "at {i}, {j}");
        }
    }

    // ColorFilter parameter on Image bitmap composable.
    let mut tinted_ui = ComposeTestRule::sized(IntSize::new(10, 10), move || {
        Image!(
            sample.clone(),
            None,
            modifier = Modifier.size(10.dp()).test_tag(TAG),
            color_filter = Some(ColorFilter::tint(Color::GREEN)),
        );
    });
    tinted_ui.on_node_with_tag(TAG).capture_to_image().assert_pixel_color(Color::GREEN, 2, 2);
}

// Compose: compose/ui/ui-graphics/src/androidDeviceTest/kotlin/androidx/compose/ui/graphics/drawscope/DrawScopeTest.kt, testDrawImageWithFilterQualityNone
#[test]
fn draw_image_with_filter_quality_modes() {
    use compose::ui::canvas::{Canvas, CanvasDrawScope};
    use compose::ui::{Density, FilterQuality, LayoutDirection, Rect};

    let sample = diagonal_sample_bitmap();
    let render_with = |quality: FilterQuality| {
        let dst = ImageBitmap::new(90, 90);
        let mut canvas = Canvas::new(&dst);
        CanvasDrawScope::draw(
            &mut canvas,
            Density::new(1.0),
            LayoutDirection::Ltr,
            Size::new(90.0, 90.0),
            |scope| {
                scope.draw_rect(Rect::new(0.0, 0.0, 90.0, 90.0), Paint::fill(Color::WHITE));
                scope.draw_image_with(
                    &sample,
                    Rect::new(0.0, 0.0, 3.0, 3.0),
                    Rect::new(0.0, 0.0, 90.0, 90.0),
                    1.0,
                    None,
                    quality,
                );
            },
        );
        dst.to_pixel_map()
    };
    let nearest = render_with(FilterQuality::None);
    assert_eq!(nearest.get(29, 29), Color::RED);
    assert_eq!(nearest.get(30, 30), Color::BLUE);
    for quality in [FilterQuality::Low, FilterQuality::Medium, FilterQuality::High] {
        // Interpolated boundary pixel between Red (0,0) and White (1,0) isn't pure Red.
        let filtered = render_with(quality);
        assert_ne!(filtered.get(29, 10), Color::RED, "quality {quality:?} should interpolate");
    }
}
