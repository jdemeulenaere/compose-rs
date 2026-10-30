//! `DrawScope::draw_text` and `draw_text_layout`, adapted from Compose's `TextPainterTest`: what
//! they draw through Skia, in the system's fonts, compared picture to picture as Compose's tests
//! do. A Compose test's bitmap with a canvas smaller than it is a window of the bitmap's size with
//! a node of the canvas's: what's drawn past the node isn't clipped by it.

use std::cell::RefCell;
use std::rc::Rc;

use compose::foundation::layout::*;
use compose::foundation::*;
use compose::runtime::*;
use compose::ui::platform::{Renderer, SharedTextMeasurer, UiHost};
use compose::ui::{
    AnnotatedString, Color, Constraints, Density, DeviceFontFamilyName, DrawScope,
    DrawTextLayoutOptions, DrawTextOptions, Font, FontFamily, FontFamilyResolver,
    FontLoadingStrategy, FontOptions, FontVariation, FontVariationSettings, IntSize,
    LOCAL_FONT_FAMILY_RESOLVER, LayoutDirection, LocaleList, Modifier, Offset, Size,
    TextDecoration, TextLayoutResult, TextMeasureOptions, TextMeasurer, TextOverflow, TextPainter,
    TextStyle, Typeface, TypefaceLoader, UnitExt, create_font_family_resolver,
    remember_text_measurer,
};
use compose_skia::{RasterRenderer, SkiaTextMeasurer};

const LONG_TEXT: &str = "Lorem ipsum dolor sit amet, consectetur \
    adipiscing elit. Curabitur augue leo, finibus vitae felis ac, pretium condimentum \
    augue. Nullam non libero sed lectus aliquet venenatis non at purus. Fusce id arcu \
    eu mauris pulvinar laoreet.";

fn style_of(font_size: f32) -> TextStyle {
    TextStyle { font_size: font_size.sp(), ..TextStyle::DEFAULT }
}

/// A picture: the pixels of a window, row by row.
#[derive(PartialEq)]
struct Picture {
    width: i32,
    pixels: Vec<Color>,
}

impl Picture {
    /// Whether nothing is drawn in the columns from `left` on and the rows from `top` on.
    fn is_empty_from(&self, left: i32, top: i32) -> bool {
        self.pixels.iter().enumerate().all(|(index, pixel)| {
            let (x, y) = (index as i32 % self.width, index as i32 / self.width);
            x < left || y < top || *pixel == Color::WHITE
        })
    }

    fn contains(&self, color: Color) -> bool {
        self.pixels.contains(&color)
    }

    fn is_blank(&self) -> bool {
        self.is_empty_from(0, 0)
    }
}

impl std::fmt::Debug for Picture {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Picture({} pixels)", self.pixels.len())
    }
}

/// What a draw block is given: the drawing and a measurer of the host's.
type Block = dyn Fn(&mut DrawScope<'_>, &TextMeasurer);

/// Draws `block` in a node of `canvas` pixels, in a window of `window` pixels at `density`, with
/// a measurer keeping `cache_size` layouts, and returns the window's picture.
fn draw_with(
    window: (i32, i32),
    canvas: (i32, i32),
    density: f32,
    cache_size: usize,
    block: impl Fn(&mut DrawScope<'_>, &TextMeasurer) + 'static,
) -> Picture {
    let block: Rc<Block> = Rc::new(block);
    let mut host = UiHost::new(
        IntSize::new(window.0, window.1),
        Density::new(density),
        Box::new(SkiaTextMeasurer::new()),
    );
    let size = (canvas.0 as f32 / density, canvas.1 as f32 / density);
    host.set_content(move || {
        let measurer = remember_text_measurer!(cache_size);
        let block = block.clone();
        Box!(
            modifier = Modifier.size((size.0.dp(), size.1.dp())).draw_behind(move |scope| {
                block(scope, &measurer);
            })
        );
    });
    host.frame();
    render(&mut host, window)
}

fn render(host: &mut UiHost, window: (i32, i32)) -> Picture {
    let mut renderer = RasterRenderer::new(window.0, window.1);
    host.with_tree(|tree| {
        renderer.render(tree.display_list(), Color::WHITE, 1.0, &*tree.text_measurer_ref())
    });
    let pixels = (0..window.1)
        .flat_map(|y| (0..window.0).map(move |x| (x, y)))
        .map(|(x, y)| renderer.pixel(x, y))
        .collect();
    Picture { width: window.0, pixels }
}

/// Draws in a square of 1000 pixels, as Compose's `draw`'s default, without a cache.
fn draw(block: impl Fn(&mut DrawScope<'_>, &TextMeasurer) + 'static) -> Picture {
    draw_with((1000, 1000), (1000, 1000), 1.0, 0, block)
}

fn sized(
    width: i32,
    height: i32,
    block: impl Fn(&mut DrawScope<'_>, &TextMeasurer) + 'static,
) -> Picture {
    draw_with((width, height), (width, height), 1.0, 0, block)
}

fn in_smaller_canvas(block: impl Fn(&mut DrawScope<'_>, &TextMeasurer) + 'static) -> Picture {
    draw_with((300, 300), (200, 200), 1.0, 0, block)
}

/// `measure`, as Compose's tests call it with the text, a style and constraints.
fn measured(
    measurer: &TextMeasurer,
    style: TextStyle,
    constraints: Constraints,
) -> TextLayoutResult {
    measurer.measure(
        LONG_TEXT,
        TextMeasureOptions { style, constraints, ..TextMeasureOptions::default() },
    )
}

fn no_overrides() -> DrawTextLayoutOptions {
    DrawTextLayoutOptions::default()
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextPainterTest.kt, drawTextWithMeasurer_shouldBeEqualTo_drawTextLayoutResult
#[test]
fn drawing_with_a_measurer_draws_what_a_measured_layout_draws() {
    let by_layout = draw(|scope, measurer| {
        let result = measured(measurer, style_of(20.0), Constraints::at_most(400, 400));
        scope.draw_text_layout(&result, Color::UNSPECIFIED, no_overrides());
    });
    let by_measurer = draw(|scope, measurer| {
        scope.draw_text(
            measurer,
            LONG_TEXT,
            DrawTextOptions {
                style: style_of(20.0),
                size: Size::new(400.0, 400.0),
                ..DrawTextOptions::default()
            },
        );
    });
    assert!(!by_layout.is_blank());
    assert_eq!(by_layout, by_measurer);
}

/// `LONG_TEXT`, at (100, 100) of a canvas of 200 in a window of 300, in `size`: at 24 sp, its
/// glyphs about as wide as Compose's test font's at 12 sp, it takes more than the 100 pixels left.
fn at_the_center(size: Size) -> Picture {
    in_smaller_canvas(move |scope, measurer| {
        scope.draw_text(
            measurer,
            LONG_TEXT,
            DrawTextOptions {
                style: style_of(24.0),
                top_left: Offset::new(100.0, 100.0),
                size,
                ..DrawTextOptions::default()
            },
        );
    })
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextPainterTest.kt, drawTextString_sizeUnspecified_shouldFitTheTextInside
#[test]
fn an_unspecified_size_fits_the_text_in_the_drawing() {
    let unconstrained = at_the_center(Size::UNSPECIFIED);
    let larger_than_the_drawing = at_the_center(Size::new(200.0, 200.0));
    let the_drawing = at_the_center(Size::new(100.0, 100.0));
    assert_ne!(unconstrained, larger_than_the_drawing);
    assert_eq!(unconstrained, the_drawing);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextPainterTest.kt, drawTextString_widthUnspecified_shouldFitTheTextHorizontally
#[test]
fn an_unspecified_width_fits_the_text_horizontally() {
    let unconstrained = at_the_center(Size::new(f32::NAN, 200.0));
    let larger_than_the_drawing = at_the_center(Size::new(200.0, 200.0));
    let the_drawing = at_the_center(Size::new(100.0, 200.0));
    assert_ne!(unconstrained, larger_than_the_drawing);
    assert_eq!(unconstrained, the_drawing);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextPainterTest.kt, drawTextString_heightUnspecified_shouldFitTheTextVertically
#[test]
fn an_unspecified_height_fits_the_text_vertically() {
    let unconstrained = at_the_center(Size::new(200.0, f32::NAN));
    let larger_than_the_drawing = at_the_center(Size::new(200.0, 200.0));
    let the_drawing = at_the_center(Size::new(200.0, 100.0));
    assert_ne!(unconstrained, larger_than_the_drawing);
    assert_eq!(unconstrained, the_drawing);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextPainterTest.kt, drawTextAnnotatedString_sizeUnspecified_shouldFitTheTextInside
#[test]
fn an_unspecified_size_fits_a_styled_text_in_the_drawing() {
    let drawn = |size| {
        in_smaller_canvas(move |scope, measurer| {
            scope.draw_text(
                measurer,
                AnnotatedString::from(LONG_TEXT),
                DrawTextOptions {
                    style: style_of(12.0),
                    top_left: Offset::new(100.0, 100.0),
                    size,
                    ..DrawTextOptions::default()
                },
            );
        })
    };
    assert_ne!(drawn(Size::UNSPECIFIED), drawn(Size::new(200.0, 200.0)));
}

/// Draws `LONG_TEXT` in `first`, then in `second`, with a measurer that keeps its layouts.
fn twice_with_a_cache(first: TextStyle, second: TextStyle) -> (Picture, Picture) {
    let mut host =
        UiHost::new(IntSize::new(500, 500), Density::new(1.0), Box::new(SkiaTextMeasurer::new()));
    let style_owner = mutable_state_of(first);
    let style = style_owner.handle();
    let shown = style;
    host.set_content(move || {
        let measurer = remember_text_measurer!();
        Box!(
            modifier = Modifier.size((500.0.dp(), 500.0.dp())).draw_behind(move |scope| {
                scope.draw_text(
                    &measurer,
                    LONG_TEXT,
                    DrawTextOptions {
                        style: shown.get(),
                        size: Size::new(400.0, 400.0),
                        ..DrawTextOptions::default()
                    },
                );
            })
        );
    });
    host.frame();
    let before = render(&mut host, (500, 500));
    style.set(second);
    host.frame();
    (before, render(&mut host, (500, 500)))
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextPainterTest.kt, textMeasurerCache_shouldNotAffectTheResult_forColor
#[test]
fn a_cached_layout_is_drawn_in_the_color_asked_for() {
    let (red, blue) = twice_with_a_cache(
        TextStyle { color: Color::RED, ..style_of(20.0) },
        TextStyle { color: Color::BLUE, ..style_of(20.0) },
    );
    assert_ne!(red, blue);
    assert!(red.contains(Color::RED) && !red.contains(Color::BLUE));
    assert!(blue.contains(Color::BLUE) && !blue.contains(Color::RED));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextPainterTest.kt, textMeasurerCache_shouldNotAffectTheResult_forFontSize
#[test]
fn a_cached_layout_is_not_drawn_at_another_font_size() {
    let (smaller, larger) = twice_with_a_cache(style_of(20.0), style_of(24.0));
    assert_ne!(smaller, larger);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextPainterTest.kt, drawTextLayout_shouldChangeColor
#[test]
fn a_layout_is_drawn_in_the_color_given() {
    let fixed = Constraints::fixed(400, 400);
    let overridden = draw(move |scope, measurer| {
        let result = measured(measurer, TextStyle { color: Color::RED, ..style_of(20.0) }, fixed);
        scope.draw_text_layout(&result, Color::BLUE, no_overrides());
    });
    let own = draw(move |scope, measurer| {
        let result = measured(measurer, TextStyle { color: Color::BLUE, ..style_of(20.0) }, fixed);
        scope.draw_text_layout(&result, Color::UNSPECIFIED, no_overrides());
    });
    assert_eq!(overridden, own);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextPainterTest.kt, drawTextLayout_shouldChangeAlphaColor
#[test]
fn a_layout_is_drawn_with_the_alpha_given() {
    let fixed = Constraints::fixed(400, 400);
    let overridden = draw(move |scope, measurer| {
        let result = measured(measurer, TextStyle { color: Color::RED, ..style_of(20.0) }, fixed);
        scope.draw_text_layout(
            &result,
            Color::UNSPECIFIED,
            DrawTextLayoutOptions { alpha: 0.5, ..no_overrides() },
        );
    });
    let own = draw(move |scope, measurer| {
        let color = Color::RED.with_alpha(0.5);
        let result = measured(measurer, TextStyle { color, ..style_of(20.0) }, fixed);
        scope.draw_text_layout(&result, Color::UNSPECIFIED, no_overrides());
    });
    assert!(!own.is_blank());
    assert_eq!(overridden, own);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextPainterTest.kt, textMeasurerDraw_isConstrainedTo_canvasSizeByDefault
#[test]
fn a_measurers_drawing_is_constrained_to_the_drawing() {
    let by_layout = sized(200, 2000, |scope, measurer| {
        let result = measured(measurer, style_of(20.0), Constraints::fixed(200, 2000));
        scope.draw_text_layout(&result, Color::UNSPECIFIED, no_overrides());
    });
    let by_measurer = sized(200, 2000, |scope, measurer| {
        scope.draw_text(
            measurer,
            LONG_TEXT,
            DrawTextOptions { style: style_of(20.0), ..DrawTextOptions::default() },
        );
    });
    assert_eq!(by_layout, by_measurer);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextPainterTest.kt, textMeasurerDraw_usesCanvasDensity_ByDefault
#[test]
fn a_measurers_drawing_is_at_the_density_of_the_drawing() {
    let by_layout = draw_with((1000, 1000), (1000, 1000), 1.0, 0, |scope, measurer| {
        let result = measurer.measure(
            LONG_TEXT,
            TextMeasureOptions {
                style: style_of(20.0),
                density: Some(Density::new(4.0)),
                constraints: Constraints::fixed(1000, 1000),
                ..TextMeasureOptions::default()
            },
        );
        scope.draw_text_layout(&result, Color::UNSPECIFIED, no_overrides());
    });
    let by_measurer = draw_with((1000, 1000), (1000, 1000), 4.0, 0, |scope, measurer| {
        scope.draw_text(
            measurer,
            LONG_TEXT,
            DrawTextOptions { style: style_of(20.0), ..DrawTextOptions::default() },
        );
    });
    assert_eq!(by_layout, by_measurer);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextPainterTest.kt, drawTextClipsTheContent_ifOverflowIsClip
#[test]
fn a_clipped_layout_draws_nothing_past_its_size() {
    let picture = sized(400, 200, |scope, measurer| {
        let result = measurer.measure(
            LONG_TEXT,
            TextMeasureOptions {
                style: style_of(14.0),
                soft_wrap: false,
                overflow: TextOverflow::Clip,
                constraints: Constraints::fixed(200, 200),
                ..TextMeasureOptions::default()
            },
        );
        scope.draw_text_layout(&result, Color::UNSPECIFIED, no_overrides());
    });
    assert!(!picture.is_blank());
    assert!(picture.is_empty_from(200, 0));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextPainterTest.kt, drawTextClipsTheContent_ifOverflowIsEllipsis_ifLessThanOneLineFits
#[test]
fn an_ellipsized_layout_less_than_a_line_high_draws_nothing_below_its_size() {
    let height = 10;
    let picture = sized(200, 200, move |scope, measurer| {
        let result = measurer.measure(
            LONG_TEXT,
            TextMeasureOptions {
                style: style_of(20.0),
                soft_wrap: false,
                overflow: TextOverflow::Ellipsis,
                constraints: Constraints::fixed(200, height),
                ..TextMeasureOptions::default()
            },
        );
        scope.draw_text_layout(&result, Color::UNSPECIFIED, no_overrides());
    });
    assert!(picture.is_empty_from(0, height));
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextPainterTest.kt, drawTextDoesNotClipTheContent_ifOverflowIsVisible
#[test]
fn a_layout_shown_past_its_size_is_not_clipped() {
    let clipped_wider = sized(400, 200, |scope, measurer| {
        let result = measurer.measure(
            LONG_TEXT,
            TextMeasureOptions {
                style: style_of(14.0),
                soft_wrap: false,
                overflow: TextOverflow::Clip,
                constraints: Constraints::fixed(400, 200),
                ..TextMeasureOptions::default()
            },
        );
        scope.draw_text_layout(&result, Color::UNSPECIFIED, no_overrides());
    });
    let not_clipped = sized(400, 200, |scope, measurer| {
        let result = measurer.measure(
            LONG_TEXT,
            TextMeasureOptions {
                style: style_of(14.0),
                soft_wrap: false,
                overflow: TextOverflow::Visible,
                constraints: Constraints::fixed(200, 200),
                ..TextMeasureOptions::default()
            },
        );
        scope.draw_text_layout(&result, Color::UNSPECIFIED, no_overrides());
    });
    assert_eq!(clipped_wider, not_clipped);
}

// Compose: compose/ui/ui-text/src/androidDeviceTest/kotlin/androidx/compose/ui/text/TextPainterTest.kt, drawTextString_sizeUnspecified_topLeftOutsideCanvas_shouldNotCrash
#[test]
fn a_top_left_outside_the_drawing_draws_without_a_panic() {
    for top_left in [(200.0, 50.0), (50.0, 200.0), (200.0, 200.0)] {
        sized(100, 100, move |scope, measurer| {
            scope.draw_text(
                measurer,
                "Hello",
                DrawTextOptions {
                    style: style_of(12.0),
                    top_left: Offset::new(top_left.0, top_left.1),
                    ..DrawTextOptions::default()
                },
            );
        });
    }
}

// ---- compose-rs's own ----------------------------------------------------------------------

#[test]
fn the_text_painter_paints_a_layout_at_the_drawings_origin() {
    let painted = draw(|scope, measurer| {
        let result = measured(measurer, style_of(20.0), Constraints::fixed(400, 400));
        TextPainter::paint(scope, &result);
    });
    let drawn = draw(|scope, measurer| {
        let result = measured(measurer, style_of(20.0), Constraints::fixed(400, 400));
        scope.draw_text_layout(&result, Color::UNSPECIFIED, no_overrides());
    });
    assert_eq!(painted, drawn);
}

#[test]
fn a_layout_is_drawn_with_the_decoration_given() {
    let overridden = draw(|scope, measurer| {
        let result = measured(measurer, style_of(20.0), Constraints::fixed(400, 400));
        scope.draw_text_layout(
            &result,
            Color::UNSPECIFIED,
            DrawTextLayoutOptions {
                text_decoration: Some(TextDecoration::UNDERLINE),
                ..no_overrides()
            },
        );
    });
    let own = draw(|scope, measurer| {
        let style =
            TextStyle { text_decoration: Some(TextDecoration::UNDERLINE), ..style_of(20.0) };
        let result = measured(measurer, style, Constraints::fixed(400, 400));
        scope.draw_text_layout(&result, Color::UNSPECIFIED, no_overrides());
    });
    assert_eq!(overridden, own);
}

#[test]
fn a_text_nodes_own_result_is_drawn_where_it_is_asked_to() {
    // What `on_text_layout` hears isn't a measurer's: it's laid out when first drawn.
    let kept: Rc<RefCell<Option<TextLayoutResult>>> = Rc::default();
    let heard_once_owner = mutable_state_of(0);
    let heard_once = heard_once_owner.handle();
    let mut host =
        UiHost::new(IntSize::new(400, 200), Density::new(1.0), Box::new(SkiaTextMeasurer::new()));
    let (heard, read) = (kept.clone(), kept.clone());
    host.set_content(move || {
        let heard = heard.clone();
        let read = read.clone();
        Column!(modifier = Modifier.fill_max_size(), {
            let heard = heard.clone();
            BasicText!(
                "Hello",
                modifier = Modifier.height(20.0.dp()),
                on_text_layout = move |result: TextLayoutResult| {
                    *heard.borrow_mut() = Some(result);
                    heard_once.set(1);
                }
            );
            let read = read.clone();
            Box!(
                modifier = Modifier.fill_max_size().draw_behind(move |scope| {
                    let _ = heard_once.get();
                    if let Some(result) = &*read.borrow() {
                        scope.draw_text_layout(result, Color::RED, no_overrides());
                    }
                })
            );
        });
    });
    host.frame();
    // A layout heard is drawn by the frame after it.
    host.frame();
    let picture = render(&mut host, (400, 200));
    assert!(picture.contains(Color::RED));
}

#[test]
fn a_result_from_another_hosts_measurer_is_laid_out_again_to_be_drawn() {
    // Each host's handles mean nothing to another's renderer.
    let foreign = {
        let mut other =
            UiHost::new(IntSize::new(10, 10), Density::new(1.0), Box::new(SkiaTextMeasurer::new()));
        let found: Rc<RefCell<Option<TextMeasurer>>> = Rc::default();
        let keep = found.clone();
        other.set_content(move || {
            *keep.borrow_mut() = Some(remember_text_measurer!(0));
        });
        other.frame();
        let measurer = found.borrow().clone().expect("composed");
        // The other host goes away; its measurer stays alive with what it measured.
        measurer.measure(
            "Hello",
            TextMeasureOptions { style: style_of(20.0), ..TextMeasureOptions::default() },
        )
    };
    let picture = sized(200, 100, move |scope, _| {
        scope.draw_text_layout(&foreign, Color::RED, no_overrides());
    });
    assert!(picture.contains(Color::RED));
}

#[test]
fn the_hosts_font_family_resolver_is_provided_and_stays() {
    let mut host =
        UiHost::new(IntSize::new(10, 10), Density::new(1.0), Box::new(SkiaTextMeasurer::new()));
    let seen: Rc<RefCell<Vec<FontFamilyResolver>>> = Rc::default();
    let kept = seen.clone();
    let tick_owner = mutable_state_of(0);
    let tick = tick_owner.handle();
    host.set_content(move || {
        let _ = tick.get();
        kept.borrow_mut().push(LOCAL_FONT_FAMILY_RESOLVER.current());
    });
    host.frame();
    tick.set(1);
    host.frame();
    let seen = seen.borrow();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0], seen[1]);
}

#[test]
fn a_remembered_measurer_stays_while_what_it_depends_on_does() {
    let mut host =
        UiHost::new(IntSize::new(10, 10), Density::new(1.0), Box::new(SkiaTextMeasurer::new()));
    let seen: Rc<RefCell<Vec<TextMeasurer>>> = Rc::default();
    let kept = seen.clone();
    let cache_size_owner = mutable_state_of(8usize);
    let cache_size = cache_size_owner.handle();
    let tick_owner = mutable_state_of(0);
    let tick = tick_owner.handle();
    host.set_content(move || {
        let _ = tick.get();
        kept.borrow_mut().push(remember_text_measurer!(cache_size.get()));
    });
    host.frame();
    tick.set(1);
    host.frame();
    cache_size.set(4);
    host.frame();
    let seen = seen.borrow();
    assert_eq!(seen.len(), 3);
    assert_eq!(seen[0], seen[1]);
    assert_ne!(seen[1], seen[2]);
}

#[test]
fn a_recording_keeps_what_it_draws_until_it_is_drawn_again() {
    // The measurer's cache is empty, so only the recording keeps the paragraph.
    let mut host =
        UiHost::new(IntSize::new(200, 100), Density::new(1.0), Box::new(SkiaTextMeasurer::new()));
    let text_owner = mutable_state_of("Hello");
    let text = text_owner.handle();
    host.set_content(move || {
        let measurer = remember_text_measurer!(0);
        Box!(
            modifier = Modifier.fill_max_size().draw_behind(move |scope| {
                scope.draw_text(
                    &measurer,
                    text.get(),
                    DrawTextOptions {
                        style: TextStyle { color: Color::RED, ..style_of(20.0) },
                        ..DrawTextOptions::default()
                    },
                );
            })
        );
    });
    let live = |host: &mut UiHost| {
        host.with_tree(|tree| {
            let measurer = tree.text_measurer_ref();
            (&*measurer as &dyn std::any::Any)
                .downcast_ref::<SkiaTextMeasurer>()
                .expect("the skia measurer")
                .live()
        })
    };
    host.frame();
    assert_eq!(live(&mut host), 1);
    // It's drawn as often as the renderer likes, with nothing but the recording holding it.
    assert!(render(&mut host, (200, 100)).contains(Color::RED));
    assert!(render(&mut host, (200, 100)).contains(Color::RED));
    // The frame replacing it gives it back, its own being the only one left.
    text.set("World");
    host.frame();
    assert!(render(&mut host, (200, 100)).contains(Color::RED));
    assert_eq!(live(&mut host), 1);
}

/// `host`'s measurer's paragraphs.
fn live_on(host: &UiHost) -> usize {
    host.with_tree(|tree| {
        let measurer = tree.text_measurer_ref();
        (&*measurer as &dyn std::any::Any)
            .downcast_ref::<SkiaTextMeasurer>()
            .expect("the skia measurer")
            .live()
    })
}

#[test]
fn what_a_removed_drawing_kept_goes_with_the_frame_removing_it_on_a_screen_gone_idle() {
    let mut host =
        UiHost::new(IntSize::new(200, 100), Density::new(1.0), Box::new(SkiaTextMeasurer::new()));
    let shown_owner = mutable_state_of(true);
    let shown = shown_owner.handle();
    host.set_content(move || {
        let measurer = remember_text_measurer!(0);
        if shown.get() {
            for index in 0..20 {
                let measurer = measurer.clone();
                Box!(
                    modifier = Modifier.size(10.dp()).draw_behind(move |scope| {
                        scope.draw_text(&measurer, format!("{index}"), DrawTextOptions::default());
                    })
                );
            }
        }
    });
    host.frame();
    assert_eq!(live_on(&host), 20);
    shown.set(false);
    host.frame();
    assert_eq!(live_on(&host), 0);
}

/// The stack of a resolver a test provides, with paragraphs of its own, so that its handles
/// name something else in the host's measurer if they end up there.
fn another_stack() -> SharedTextMeasurer {
    let stack = SharedTextMeasurer::new(Box::new(SkiaTextMeasurer::new()));
    for text in ["one", "two", "three"] {
        let handle = stack.borrow_mut().layout(
            text,
            &TextStyle::DEFAULT,
            Default::default(),
            Constraints::unbounded(),
            &Density::new(1.0),
        );
        // Kept: the handles the measurer gives next are others'.
        let _ = handle;
    }
    stack
}

#[test]
fn a_resolver_over_another_stack_draws_on_the_trees() {
    let stack = another_stack();
    let provided = create_font_family_resolver(&stack);
    let mut host =
        UiHost::new(IntSize::new(300, 100), Density::new(1.0), Box::new(SkiaTextMeasurer::new()));
    host.set_content(move || {
        CompositionLocalProvider(LOCAL_FONT_FAMILY_RESOLVER.provides(provided.clone()), || {
            let measurer = remember_text_measurer!(0);
            Box!(
                modifier = Modifier.fill_max_size().draw_behind(move |scope| {
                    scope.draw_text(
                        &measurer,
                        "Hello",
                        DrawTextOptions {
                            style: TextStyle { color: Color::RED, ..style_of(40.0) },
                            ..DrawTextOptions::default()
                        },
                    );
                })
            );
        });
    });
    host.frame();
    assert!(render(&mut host, (300, 100)).contains(Color::RED));
    // Measured on the other stack, drawn from a paragraph of the tree's.
    assert_eq!(live_on(&host), 1);
}

#[test]
fn a_result_drawn_by_two_hosts_alternately_is_drawn_by_each() {
    let stack = another_stack();
    let result = TextMeasurer::new(
        create_font_family_resolver(&stack),
        LocaleList::empty(),
        Density::new(1.0),
        LayoutDirection::Ltr,
        0,
    )
    .measure(
        "Hello",
        TextMeasureOptions { style: style_of(40.0), ..TextMeasureOptions::default() },
    );
    let tick_owner = mutable_state_of(0u32);
    let tick = tick_owner.handle();
    let host_of = |result: TextLayoutResult| {
        let host = UiHost::new(
            IntSize::new(300, 100),
            Density::new(1.0),
            Box::new(SkiaTextMeasurer::new()),
        );
        host.set_content(move || {
            let result = result.clone();
            Box!(
                modifier = Modifier.fill_max_size().draw_behind(move |scope| {
                    let _ = tick.get();
                    scope.draw_text_layout(&result, Color::RED, no_overrides());
                })
            );
        });
        host
    };
    let (mut first, mut second) = (host_of(result.clone()), host_of(result));
    for step in 1..5 {
        first.frame();
        assert!(render(&mut first, (300, 100)).contains(Color::RED), "step {step}");
        tick.set(step * 2);
        second.frame();
        assert!(render(&mut second, (300, 100)).contains(Color::RED), "step {step}");
        tick.set(step * 2 + 1);
    }
}

#[test]
fn drawing_a_layout_twice_in_another_color_keeps_one_repainted_paragraph() {
    let picture = draw(|scope, measurer| {
        let result = measurer.measure(
            "Hello",
            TextMeasureOptions { style: style_of(40.0), ..TextMeasureOptions::default() },
        );
        scope.draw_text_layout(&result, Color::RED, no_overrides());
        scope.draw_text_layout(
            &result,
            Color::BLUE,
            DrawTextLayoutOptions { top_left: Offset::new(0.0, 100.0), ..no_overrides() },
        );
    });
    assert!(picture.contains(Color::RED) && picture.contains(Color::BLUE));
    // Both are drawn from the same frame's recording: the first isn't lost to the second.
    let first_rows = &picture.pixels[..100 * 1000];
    assert!(first_rows.contains(&Color::RED) && !first_rows.contains(&Color::BLUE));
}

// ---- Fonts on the real stack -------------------------------------------------------------------

/// A monospaced font file: the sample font's (D1124), in the target directory.
fn monospaced_font_file() -> &'static str {
    static FILE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    FILE.get_or_init(|| {
        let path = format!("{}/sample_font.ttf", env!("CARGO_TARGET_TMPDIR"));
        std::fs::write(&path, compose::test::basic_measure_font_bytes())
            .expect("writing the sample font");
        path
    })
}

/// A variable font file of this system, with a `wght` axis. A system with none of these fails
/// here, rather than these tests passing with nothing asserted.
fn variable_font_file() -> &'static str {
    [
        "/System/Library/Fonts/SFNS.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans[wght].ttf",
        "/usr/share/fonts/noto/NotoSans[wght].ttf",
    ]
    .into_iter()
    .find(|path| std::path::Path::new(path).exists())
    .expect("these tests need a variable font file: install Noto Sans")
}

fn width_of(text: &str, family: &FontFamily) -> i32 {
    let shared = SharedTextMeasurer::new(Box::new(SkiaTextMeasurer::new()));
    let measurer = TextMeasurer::new(
        create_font_family_resolver(&shared),
        LocaleList::empty(),
        Density::new(1.0),
        LayoutDirection::Ltr,
        0,
    );
    let style = TextStyle { font_family: Some(family.clone()), ..style_of(40.0) };
    let result =
        measurer.measure(text, TextMeasureOptions { style, ..TextMeasureOptions::default() });
    result.size.width
}

/// A picture 50 pixels square of `text` in the sample font at 50 pixels.
fn sample_font_picture(text: &'static str) -> Picture {
    sized(50, 50, move |scope, measurer| {
        let style = TextStyle {
            font_family: Some(compose::test::basic_measure_font().to_font_family()),
            color: Color::BLACK,
            ..style_of(50.0)
        };
        scope.draw_text(measurer, text, DrawTextOptions { style, ..DrawTextOptions::default() });
    })
}

#[test]
fn the_sample_fonts_glyphs_point_along_their_direction() {
    let ink_at = |picture: &Picture, x: i32, y: i32| {
        picture.pixels[(y * picture.width + x) as usize] != Color::WHITE
    };
    let latin = sample_font_picture("a");
    assert!(ink_at(&latin, 2, 3), "the top of a triangle pointing right is at its left");
    assert!(!ink_at(&latin, 47, 3));
    assert!(ink_at(&latin, 47, 25), "and its point at its right");
    let hebrew = sample_font_picture("\u{5D0}");
    assert!(!ink_at(&hebrew, 2, 3));
    assert!(ink_at(&hebrew, 47, 3), "the top of a triangle pointing left is at its right");
    assert!(ink_at(&hebrew, 2, 25), "and its point at its left");
}

#[test]
fn a_font_of_a_file_is_what_a_text_is_laid_out_in() {
    let path = monospaced_font_file();
    let family = Font::from_file(path, FontOptions::default()).to_font_family();
    assert_eq!(width_of("iiiiiiii", &family), width_of("WWWWWWWW", &family));
    // Not the default family's, which is proportional.
    assert_ne!(
        width_of("iiiiiiii", &FontFamily::SansSerif),
        width_of("WWWWWWWW", &FontFamily::SansSerif)
    );
}

#[test]
fn a_font_of_bytes_is_what_a_text_is_laid_out_in() {
    let path = monospaced_font_file();
    let bytes: std::sync::Arc<[u8]> = std::fs::read(path).expect("the font").into();
    let family = Font::from_shared_bytes(bytes, FontOptions::default()).to_font_family();
    assert_eq!(width_of("iiiiiiii", &family), width_of("WWWWWWWW", &family));
}

#[test]
fn a_blocking_font_that_cannot_be_read_is_the_default() {
    let family = Font::from_file("/no/such/font.ttf", FontOptions::default()).to_font_family();
    assert_eq!(width_of("Hello", &family), width_of("Hello", &FontFamily::SansSerif));
}

#[test]
fn a_device_family_that_is_missing_goes_on_to_the_next_font() {
    let path = monospaced_font_file();
    let missing = Font::from_device_family(
        DeviceFontFamilyName::new("No Such Family"),
        FontOptions::default(),
    );
    let family = FontFamily::of([missing, Font::from_file(path, FontOptions::default())]);
    assert_eq!(width_of("iiiiiiii", &family), width_of("WWWWWWWW", &family));
}

#[test]
fn a_span_in_a_font_of_its_own_is_laid_out_in_it() {
    let path = monospaced_font_file();
    let family = Font::from_file(path, FontOptions::default()).to_font_family();
    let in_font = |text: &'static str| {
        AnnotatedString::build(|builder| {
            builder.with_style(
                compose::ui::SpanStyle {
                    font_family: Some(family.clone()),
                    ..compose::ui::SpanStyle::DEFAULT
                },
                |builder| {
                    builder.append(text);
                },
            );
        })
    };
    let shared = SharedTextMeasurer::new(Box::new(SkiaTextMeasurer::new()));
    let measurer = TextMeasurer::new(
        create_font_family_resolver(&shared),
        LocaleList::empty(),
        Density::new(1.0),
        LayoutDirection::Ltr,
        0,
    );
    let measure = |text| {
        let options = TextMeasureOptions { style: style_of(40.0), ..TextMeasureOptions::default() };
        measurer.measure(in_font(text), options).size.width
    };
    assert_eq!(measure("iiiiiiii"), measure("WWWWWWWW"));
}

/// `text`'s width in `path` at `wght`.
fn width_at_weight(path: &str, weight: i32) -> i32 {
    let settings = FontVariationSettings::new([FontVariation::weight(weight)]);
    let font = Font::from_file(
        path,
        FontOptions { variation_settings: Some(settings), ..FontOptions::default() },
    );
    width_of("Hamburgefonstiv", &font.to_font_family())
}

#[test]
fn a_variable_font_is_laid_out_at_its_settings() {
    let path = variable_font_file();
    assert!(width_at_weight(path, 900) > width_at_weight(path, 100));
}

#[test]
fn an_animated_variation_registers_a_bounded_number_of_typefaces() {
    let path = variable_font_file();
    let mut measurer = SkiaTextMeasurer::new();
    let density = Density::new(1.0);
    for weight in 1..=300 {
        let settings = FontVariationSettings::new([FontVariation::weight(weight)]);
        let font = Font::from_file(
            path,
            FontOptions { variation_settings: Some(settings), ..FontOptions::default() },
        );
        let style = TextStyle { font_family: Some(font.to_font_family()), ..style_of(40.0) };
        let handle = compose::ui::platform::TextMeasurer::layout(
            &mut measurer,
            "Hello",
            &style,
            Default::default(),
            Constraints::unbounded(),
            &density,
        );
        compose::ui::platform::TextMeasurer::release(&mut measurer, handle);
    }
    assert!(measurer.registered_typefaces() <= 64, "{}", measurer.registered_typefaces());
}

#[test]
fn a_paragraph_outlives_the_typefaces_being_registered_anew() {
    let path = monospaced_font_file();
    let mut measurer = SkiaTextMeasurer::new();
    let density = Density::new(1.0);
    let family = Font::from_file(path, FontOptions::default()).to_font_family();
    let style = TextStyle { font_family: Some(family), ..style_of(40.0) };
    let first = compose::ui::platform::TextMeasurer::layout(
        &mut measurer,
        "iiiiiiii",
        &style,
        Default::default(),
        Constraints::unbounded(),
        &density,
    );
    // Other fonts, past the bound.
    let variable = variable_font_file();
    for weight in 1..=70 {
        let settings = FontVariationSettings::new([FontVariation::weight(weight)]);
        let font = Font::from_file(
            variable,
            FontOptions { variation_settings: Some(settings), ..FontOptions::default() },
        );
        let style = TextStyle { font_family: Some(font.to_font_family()), ..style_of(40.0) };
        let handle = compose::ui::platform::TextMeasurer::layout(
            &mut measurer,
            "Hello",
            &style,
            Default::default(),
            Constraints::unbounded(),
            &density,
        );
        compose::ui::platform::TextMeasurer::release(&mut measurer, handle);
    }
    // It answers still, and a text in its font asks for it again.
    let width = compose::ui::platform::TextMeasurer::metrics(&measurer, first).size.width;
    let again = compose::ui::platform::TextMeasurer::layout(
        &mut measurer,
        "WWWWWWWW",
        &style,
        Default::default(),
        Constraints::unbounded(),
        &density,
    );
    assert_eq!(compose::ui::platform::TextMeasurer::metrics(&measurer, again).size.width, width);
}

/// A font that arrives when asked, from `path`.
struct Arriving {
    path: &'static str,
    now: Rc<std::cell::Cell<bool>>,
}

impl TypefaceLoader for Arriving {
    fn load_blocking(
        &self,
        _: &dyn compose::ui::platform::PlatformFontLoader,
        _: &Font,
    ) -> Result<Option<Typeface>, String> {
        Err("async".to_string())
    }

    fn await_load(
        self: Rc<Self>,
        platform: Rc<dyn compose::ui::platform::PlatformFontLoader>,
        _: Font,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<Typeface>, String>>>>
    {
        Box::pin(async move {
            while !self.now.get() {
                delay(std::time::Duration::from_millis(5)).await;
            }
            platform.load_blocking(&Font::from_file(self.path, FontOptions::default()))
        })
    }
}

fn run_until(host: &mut UiHost, done: impl Fn(&UiHost) -> bool) {
    for _ in 0..200 {
        if done(host) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        host.frame();
    }
}

#[test]
fn a_parent_of_intrinsic_width_follows_a_font_arriving() {
    // The text is 16 thin letters in the default font; in the monospaced one that arrives, wide.
    let path = monospaced_font_file();
    let now = Rc::new(std::cell::Cell::new(false));
    let font = Font::from_loader(
        Rc::new(Arriving { path, now: now.clone() }),
        FontOptions {
            loading_strategy: Some(FontLoadingStrategy::Async),
            ..FontOptions::default()
        },
    );
    let style = TextStyle { font_family: Some(font.to_font_family()), ..style_of(40.0) };
    let heard: Rc<RefCell<Option<TextLayoutResult>>> = Rc::default();
    let keep = heard.clone();
    let mut host =
        UiHost::new(IntSize::new(2000, 800), Density::new(1.0), Box::new(SkiaTextMeasurer::new()));
    host.set_content(move || {
        let keep = keep.clone();
        let style = style.clone();
        Box!(modifier = Modifier.width(compose::ui::IntrinsicSize::Max), {
            let keep = keep.clone();
            BasicText!(
                "iiiiiiiiiiiiiiii",
                style = style.clone(),
                on_text_layout = move |result: TextLayoutResult| *keep.borrow_mut() = Some(result)
            );
        });
    });
    host.frame();
    let before = heard.borrow().clone().expect("laid out");
    assert_eq!(before.line_count, 1);
    now.set(true);
    let arrived = heard.clone();
    let first_width = before.size.width;
    run_until(&mut host, move |_| {
        arrived.borrow().as_ref().is_some_and(|result| result.size.width != first_width)
    });
    host.frame();
    let after = heard.borrow().clone().expect("laid out");
    assert!(after.size.width > before.size.width, "{} {}", after.size.width, before.size.width);
    assert_eq!(after.line_count, 1, "wrapped at the old font's width");
}

#[test]
fn an_async_font_of_a_file_does_not_stop_the_first_frame() {
    let path = monospaced_font_file();
    let font = Font::from_file(
        path,
        FontOptions {
            loading_strategy: Some(FontLoadingStrategy::Async),
            ..FontOptions::default()
        },
    );
    let style = TextStyle { font_family: Some(font.to_font_family()), ..style_of(40.0) };
    let heard: Rc<RefCell<Option<TextLayoutResult>>> = Rc::default();
    let keep = heard.clone();
    let mut host =
        UiHost::new(IntSize::new(2000, 800), Density::new(1.0), Box::new(SkiaTextMeasurer::new()));
    host.set_content(move || {
        let keep = keep.clone();
        BasicText!(
            "iiiiiiiiiiiiiiii",
            style = style.clone(),
            on_text_layout = move |result: TextLayoutResult| *keep.borrow_mut() = Some(result)
        );
    });
    host.frame();
    let first = heard.borrow().clone().expect("laid out").size.width;
    let mono = width_of(
        "iiiiiiiiiiiiiiii",
        &Font::from_file(path, FontOptions::default()).to_font_family(),
    );
    // The fallback, proportional, in the first frame: the worker has only begun.
    assert_ne!(first, mono);
    let arrived = heard.clone();
    run_until(&mut host, move |_| {
        arrived.borrow().as_ref().is_some_and(|result| result.size.width == mono)
    });
    assert_eq!(heard.borrow().as_ref().expect("laid out").size.width, mono);
}
