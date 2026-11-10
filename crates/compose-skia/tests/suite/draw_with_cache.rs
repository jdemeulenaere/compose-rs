//! Compose's `DrawModifierTest` (ui, device tests, the one in `androidx.compose.ui.draw`), its
//! `drawWithCache` part: what builds the cache again (a new size, direction or density, or state
//! read while building it) and what doesn't (state read while drawing), drawn by Skia and checked
//! pixel by pixel. Density 1 but where a test provides another.

use std::cell::Cell;
use std::rc::Rc;

use compose::runtime::*;
use compose::test::*;
use compose::ui::{
    BlendMode, Color, Density, IntSize, LOCAL_DENSITY, LOCAL_LAYOUT_DIRECTION, Layout,
    LayoutDirection, Modifier, Paint, Path, Rect, Size, UnitExt, measure_policy,
};

const TAG: &str = "testTag";

type Count = Rc<Cell<u32>>;

fn bump(count: &Count) {
    count.set(count.get() + 1);
}

/// At least `size` pixels each way, as the test utilities' `AtLeastSize` (with no children).
fn at_least_size(size: i32, modifier: Modifier) {
    Layout(
        modifier,
        measure_policy(move |scope, _, constraints| {
            scope.layout(size.max(constraints.min_width), size.max(constraints.min_height))
        }),
        callback(|| {}),
    );
}

fn rule(size: i32, content: impl Fn() + 'static) -> ComposeTestRule {
    set_capture_renderer(compose_skia::raster_colors);
    ComposeTestRule::sized(IntSize::new(size, size), content)
}

fn capture(ui: &mut ComposeTestRule) -> CapturedImage {
    ui.wait_for_idle();
    ui.on_node_with_tag(TAG).capture_to_image()
}

/// The rectangle from the origin to `size`, as a path.
fn rect_path(size: Size) -> Path {
    let path = Path::new();
    path.line_to(size.width, 0.0);
    path.line_to(size.width, size.height);
    path.line_to(0.0, size.height);
    path.close();
    path
}

/// The test's path from the origin to the width, then to the height at the height (a square of
/// the drawing's size, as it's square).
fn square_path(size: Size) -> Path {
    let path = Path::new();
    path.line_to(size.width, 0.0);
    path.line_to(size.height, size.height);
    path.line_to(0.0, size.height);
    path.close();
    path
}

#[track_caller]
fn assert_pixels(image: &CapturedImage, color: Color, at: &[(i32, i32)]) {
    for &(x, y) in at {
        image.assert_pixel_color(color, x, y);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/DrawModifierTest.kt, testLayoutDirectionChangeInvalidatesDrawWithCache
#[test]
fn test_layout_direction_change_invalidates_draw_with_cache() {
    let resolved = Rc::new(Cell::new(None));
    let drawn = Rc::new(Cell::new(None));
    let provided = mutable_state_of(LayoutDirection::Ltr);
    let (r, d, p) = (resolved.clone(), drawn.clone(), provided.handle());
    // Compose's `Button` with a `Text`: a clickable leaf drawing with a cache. Its modifier is
    // made once, so only the direction changing builds the cache again (not a new element).
    let toggle = move || {
        p.set(match p.get() {
            LayoutDirection::Ltr => LayoutDirection::Rtl,
            LayoutDirection::Rtl => LayoutDirection::Ltr,
        })
    };
    let cached = Modifier.draw_with_cache(move |cache| {
        r.set(Some(cache.layout_direction()));
        let d = d.clone();
        cache.on_draw_behind(move |scope| d.set(Some(scope.layout_direction())))
    });
    let modifier = Modifier.test_tag(TAG).clickable(toggle).then(cached);
    let mut ui = rule(200, move || {
        let modifier = modifier.clone();
        CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(p.get()), move || {
            at_least_size(50, modifier.clone());
        });
    });
    ui.wait_for_idle();
    assert_eq!(
        (resolved.get(), drawn.get()),
        (Some(LayoutDirection::Ltr), Some(LayoutDirection::Ltr))
    );
    ui.on_node_with_tag(TAG).perform_click();
    ui.wait_for_idle();
    assert_eq!(
        (resolved.get(), drawn.get()),
        (Some(LayoutDirection::Rtl), Some(LayoutDirection::Rtl))
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/DrawModifierTest.kt, testDensityChangeInvalidatesDrawWithCache
#[test]
fn test_density_change_invalidates_draw_with_cache() {
    let resolved = Rc::new(Cell::new(None));
    let drawn = Rc::new(Cell::new(None));
    let provided = mutable_state_of(Density::with_font_scale(2.0, 2.0));
    let (r, d, p) = (resolved.clone(), drawn.clone(), provided.handle());
    // Made once, so only the density changing builds the cache again (not a new element).
    let toggle = move || {
        let next = if p.get().density == 2.0 { 3.0 } else { 2.0 };
        p.set(Density::with_font_scale(next, next));
    };
    let cached = Modifier.draw_with_cache(move |cache| {
        r.set(Some(cache.density()));
        let d = d.clone();
        cache.on_draw_behind(move |scope| d.set(Some(scope.density())))
    });
    let modifier = Modifier.test_tag(TAG).clickable(toggle).then(cached);
    let mut ui = rule(200, move || {
        let modifier = modifier.clone();
        CompositionLocalProvider(LOCAL_DENSITY.provides(p.get()), move || {
            at_least_size(50, modifier.clone());
        });
    });
    ui.wait_for_idle();
    assert_eq!(
        (resolved.get(), drawn.get()),
        (Some(Density::with_font_scale(2.0, 2.0)), Some(Density::with_font_scale(2.0, 2.0)))
    );
    ui.on_node_with_tag(TAG).perform_click();
    ui.wait_for_idle();
    assert_eq!(
        (resolved.get(), drawn.get()),
        (Some(Density::with_font_scale(3.0, 3.0)), Some(Density::with_font_scale(3.0, 3.0)))
    );
}

/// A state read while drawing (the rectangle's color) draws again without building the cache
/// (the path) again.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/DrawModifierTest.kt, testCacheHitWithStateChange
#[test]
fn test_cache_hit_with_state_change() {
    let builds = Count::default();
    let b = builds.clone();
    let mut ui = rule(200, move || {
        let b = b.clone();
        let rect_color = remember(|| mutable_state_of(Color::BLUE));
        let cached = Modifier.draw_with_cache(move |cache| {
            let size = cache.size();
            let path = rect_path(Size::new(size.width / 2.0, size.height));
            bump(&b);
            cache.on_draw_behind(move |scope| {
                let bounds = scope.bounds();
                scope.draw_rect(bounds, Paint::fill(rect_color.get()));
                scope.draw_path(&path, Paint::fill(Color::RED));
            })
        });
        let toggle = move || {
            let next = if rect_color.get() == Color::BLUE { Color::GREEN } else { Color::BLUE };
            rect_color.set(next);
        };
        at_least_size(200, Modifier.test_tag(TAG).then(cached).clickable(toggle));
    });
    let image = capture(&mut ui);
    assert_eq!(builds.get(), 1);
    let (w, h) = (image.width(), image.height());
    let left = [(1, 1), (w / 2 - 2, 1), (w / 2 - 2, h / 2 - 2), (1, h / 2 - 2)];
    let right = [(w / 2 + 1, 1), (w - 2, 1), (w / 2 + 1, h - 2), (w - 2, h - 2)];
    assert_pixels(&image, Color::RED, &left);
    assert_pixels(&image, Color::BLUE, &right);
    ui.on_node_with_tag(TAG).perform_click();
    let image = capture(&mut ui);
    assert_eq!(builds.get(), 1, "the path was built once");
    assert_pixels(&image, Color::RED, &left);
    assert_pixels(&image, Color::GREEN, &right);
}

/// A state read while building the cache builds it again.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/DrawModifierTest.kt, invalidationForDrawWithCache
#[test]
fn invalidation_for_draw_with_cache() {
    let size = mutable_state_of(10.0f32);
    let draws = Count::default();
    let (s, d) = (size.handle(), draws.clone());
    let mut ui = rule(200, move || {
        let d = d.clone();
        let modifier = Modifier
            .graphics_layer(|_| {})
            .size(50.dp())
            .draw_with_cache(move |cache| {
                let rect = Rect::new(0.0, 0.0, s.get(), s.get());
                let d = d.clone();
                cache.on_draw_behind(move |scope| {
                    scope.draw_rect(rect, Paint::fill(Color::BLUE));
                    bump(&d);
                })
            })
            .graphics_layer(|_| {});
        // Compose's two boxes: the outer filling the window, the inner sized by its modifier.
        let inner = modifier.clone();
        Layout(
            Modifier.fill_max_size(),
            measure_policy(|scope, measurables, constraints| {
                let placeable = scope.measure(measurables[0], constraints.loosen());
                scope.place(&placeable, 0, 0);
                scope.layout(constraints.max_width, constraints.max_height)
            }),
            callback(move || at_least_size(0, inner.clone())),
        );
    });
    ui.wait_for_idle();
    assert_eq!(draws.get(), 1);
    size.set(15.0);
    ui.wait_for_idle();
    assert_eq!(draws.get(), 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/DrawModifierTest.kt, testCacheInvalidatedAfterStateChange
#[test]
fn test_cache_invalidated_after_state_change() {
    let builds = Count::default();
    let b = builds.clone();
    let mut ui = rule(200, move || {
        let b = b.clone();
        let fill_bounds = remember(|| mutable_state_of(false));
        let cached = Modifier.draw_with_cache(move |cache| {
            let size = cache.size();
            let path_size = if fill_bounds.get() {
                size
            } else {
                Size::new(size.width / 2.0, size.height / 2.0)
            };
            let path = rect_path(path_size);
            bump(&b);
            cache.on_draw_behind(move |scope| {
                let bounds = scope.bounds();
                scope.draw_rect(bounds, Paint::fill(Color::RED));
                scope.draw_path(&path, Paint::fill(Color::BLUE));
            })
        });
        let toggle = move || fill_bounds.set(!fill_bounds.get());
        at_least_size(200, Modifier.test_tag(TAG).then(cached).clickable(toggle));
    });
    let image = capture(&mut ui);
    assert_eq!(builds.get(), 1);
    let (w, h) = (image.width(), image.height());
    assert_pixels(
        &image,
        Color::BLUE,
        &[(1, 1), (w / 2 - 2, 1), (w / 2 - 2, h / 2 - 2), (1, h / 2 - 1)],
    );
    assert_pixels(
        &image,
        Color::RED,
        &[
            (w / 2 + 1, 1),
            (w / 2 + 1, h / 2 - 1),
            (w / 2 + 1, h / 2 - 2),
            (w / 2 - 2, h / 2 + 1),
            (1, h / 2 + 1),
            (1, h - 2),
            (w - 2, 1),
            (w - 2, h - 2),
        ],
    );
    ui.on_node_with_tag(TAG).perform_click();
    let image = capture(&mut ui);
    assert_eq!(builds.get(), 2);
    assert_pixels(&image, Color::BLUE, &[(1, 1), (198, 1), (198, 198), (1, 198)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/DrawModifierTest.kt, testCacheInvalidatedAfterSizeChange
#[test]
fn test_cache_invalidated_after_size_change() {
    let builds = Count::default();
    let b = builds.clone();
    let (start, end) = (200, 400);
    let size = mutable_state_of(start);
    let s = size.handle();
    // Made once, so only the new size builds the cache again (not a new element).
    let cached = Modifier.draw_with_cache(move |cache| {
        let path = square_path(cache.size());
        bump(&b);
        cache.on_draw_behind(move |scope| scope.draw_path(&path, Paint::fill(Color::RED)))
    });
    let toggle = move || s.set(if s.get() == start { end } else { start });
    let modifier = Modifier.test_tag(TAG).then(cached).clickable(toggle);
    let mut ui = rule(400, move || at_least_size(s.get(), modifier.clone()));
    let image = capture(&mut ui);
    assert_eq!(builds.get(), 1);
    assert_eq!(image.size(), IntSize::new(start, start));
    assert_pixels(&image, Color::RED, &[(1, 1), (start - 2, start - 2)]);
    ui.on_node_with_tag(TAG).perform_click();
    let image = capture(&mut ui);
    assert_eq!(builds.get(), 2);
    assert_eq!(image.size(), IntSize::new(end, end));
    assert_pixels(&image, Color::RED, &[(1, 1), (end - 2, end - 2)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/DrawModifierTest.kt, testCacheInvalidatedAfterLayoutDirectionChange
#[test]
fn test_cache_invalidated_after_layout_direction_change() {
    let direction = mutable_state_of(LayoutDirection::Ltr);
    let real = Rc::new(Cell::new(None));
    let (d, r) = (direction.handle(), real.clone());
    // Made once, so only the direction changing builds the cache again (not a new element).
    let cached = Modifier.draw_with_cache(move |cache| {
        r.set(Some(cache.layout_direction()));
        cache.on_draw_behind(|_| {})
    });
    let mut ui = rule(100, move || {
        let cached = cached.clone();
        CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(d.get()), move || {
            at_least_size(10, cached.clone());
        });
    });
    ui.wait_for_idle();
    assert_eq!(real.get(), Some(LayoutDirection::Ltr));
    direction.set(LayoutDirection::Rtl);
    ui.wait_for_idle();
    assert_eq!(real.get(), Some(LayoutDirection::Rtl));
}

/// `draw_with_cache` inside another modifier's function, given a new color by state.
fn draw_path_helper_modifier(modifier: Modifier, color: Color) -> Modifier {
    modifier.then(Modifier.draw_with_cache(move |cache| {
        let path = square_path(cache.size());
        cache.on_draw_behind(move |scope| scope.draw_path(&path, Paint::fill(color)))
    }))
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/DrawModifierTest.kt, testCacheInvalidatedWithHelperModifier
#[test]
fn test_cache_invalidated_with_helper_modifier() {
    let mut ui = rule(200, || {
        let color = remember(|| mutable_state_of(Color::RED));
        let toggle =
            move || color.set(if color.get() == Color::RED { Color::BLUE } else { Color::RED });
        let modifier = draw_path_helper_modifier(Modifier.test_tag(TAG), color.get());
        at_least_size(200, modifier.clickable(toggle));
    });
    let image = capture(&mut ui);
    assert_pixels(&image, Color::RED, &[(1, 1), (198, 198)]);
    ui.on_node_with_tag(TAG).perform_click();
    let image = capture(&mut ui);
    assert_pixels(&image, Color::BLUE, &[(1, 1), (198, 198)]);
}

/// A 200 pixel square drawing with `cached` before a blue background.
fn over_blue(cached: Modifier) -> ComposeTestRule {
    let cached = Rc::new(cached);
    rule(200, move || {
        at_least_size(200, Modifier.test_tag(TAG).then((*cached).clone()).background(Color::BLUE))
    })
}

/// The red half behind the content: the content (blue) is drawn over it without being asked.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/DrawModifierTest.kt, testDrawWithCacheContentDrawnImplicitly
#[test]
fn test_draw_with_cache_content_drawn_implicitly() {
    let mut ui = over_blue(Modifier.draw_with_cache(|cache| {
        cache.on_draw_behind(|scope| {
            let size = scope.size();
            scope.draw_rect(Rect::new(0.0, 0.0, size.width / 2.0, size.height), {
                Paint::fill(Color::RED)
            });
        })
    }));
    let image = capture(&mut ui);
    assert_pixels(&image, Color::BLUE, &[(0, 0), (199, 0), (199, 199), (0, 199)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/DrawModifierTest.kt, testDrawWithCacheOverContent
#[test]
fn test_draw_with_cache_over_content() {
    let mut ui = over_blue(Modifier.draw_with_cache(|cache| {
        cache.on_draw_with_content(|scope| {
            scope.draw_content();
            let size = scope.size();
            scope.draw_rect(Rect::new(0.0, 0.0, size.width / 2.0, size.height), {
                Paint::fill(Color::RED)
            });
        })
    }));
    let image = capture(&mut ui);
    assert_pixels(&image, Color::BLUE, &[(101, 0), (199, 0), (199, 199), (101, 199)]);
    assert_pixels(&image, Color::RED, &[(0, 0), (99, 0), (99, 199), (0, 199)]);
}

/// Green added to the blue content: cyan.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/DrawModifierTest.kt, testDrawWithCacheBlendsContent
#[test]
fn test_draw_with_cache_blends_content() {
    let mut ui = over_blue(Modifier.draw_with_cache(|cache| {
        cache.on_draw_with_content(|scope| {
            scope.draw_content();
            let bounds = scope.bounds();
            scope.draw_rect(bounds, Paint::fill(Color::GREEN).with_blend_mode(BlendMode::Plus));
        })
    }));
    let image = capture(&mut ui);
    assert_pixels(&image, Color::CYAN, &[(0, 0), (199, 0), (199, 199), (0, 199)]);
}

/// A 100 × 100 leaf with `modifier`, composed again when a state changes, the modifier the
/// same each time (made once, as Compose's lambda is one object); how many times `counter`
/// was bumped before and after.
fn recomposed_with(modifier: Modifier, counter: &Count) -> (u32, u32) {
    let recomposition = mutable_state_of(0);
    let r = recomposition.handle();
    let mut ui = rule(200, move || {
        r.get();
        Layout(
            modifier.clone(),
            measure_policy(|scope, _, _| scope.layout(100, 100)),
            callback(|| {}),
        );
    });
    ui.wait_for_idle();
    let before = counter.get();
    recomposition.set(1);
    ui.wait_for_idle();
    (before, counter.get())
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/DrawModifierTest.kt, recompositionWithTheSameDrawBehindLambdaIsNotTriggeringRedraw
#[test]
fn recomposition_with_the_same_draw_behind_lambda_is_not_triggering_redraw() {
    let draws = Count::default();
    let d = draws.clone();
    let modifier = Modifier.draw_behind(move |_| bump(&d));
    assert_eq!(recomposed_with(modifier, &draws), (1, 1));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/DrawModifierTest.kt, recompositionWithTheSameDrawWithContentLambdaIsNotTriggeringRedraw
#[test]
fn recomposition_with_the_same_draw_with_content_lambda_is_not_triggering_redraw() {
    let draws = Count::default();
    let d = draws.clone();
    let modifier = Modifier.draw_with_content(move |_| bump(&d));
    assert_eq!(recomposed_with(modifier, &draws), (1, 1));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/DrawModifierTest.kt, recompositionWithTheSameDrawWithCacheLambdaIsNotTriggeringRedraw
#[test]
fn recomposition_with_the_same_draw_with_cache_lambda_is_not_triggering_redraw() {
    let (builds, draws) = (Count::default(), Count::default());
    let (b, d) = (builds.clone(), draws.clone());
    let modifier = Modifier.draw_with_cache(move |cache| {
        bump(&b);
        let d = d.clone();
        cache.on_draw_behind(move |_| bump(&d))
    });
    assert_eq!(recomposed_with(modifier, &draws), (1, 1));
    assert_eq!(builds.get(), 1);
}

/// A state read building the cache of a node with a layer, the state set by another node: the
/// cache is built again.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/DrawModifierTest.kt, testGraphicsLayerCacheInvalidatedAfterStateChange
#[test]
fn test_graphics_layer_cache_invalidated_after_state_change() {
    let builds = Count::default();
    let b = builds.clone();
    let mut ui = rule(200, move || {
        let b = b.clone();
        let flag = remember(|| mutable_state_of(false));
        let cached =
            Modifier.test_tag("boxTag").graphics_layer(|_| {}).draw_with_cache(move |cache| {
                let color = if flag.get() { Color::RED } else { Color::BLUE };
                bump(&b);
                cache.on_draw_behind(move |scope| {
                    let bounds = scope.bounds();
                    scope.draw_rect(bounds, Paint::fill(color));
                })
            });
        let clicked =
            Modifier.test_tag("clickTag").size(20.dp()).clickable(move || flag.set(!flag.get()));
        // Compose's `Column`: the two one under the other.
        Layout(
            Modifier,
            measure_policy(|scope, measurables, constraints| {
                let (mut width, mut height) = (0, 0);
                for &measurable in measurables {
                    let placeable = scope.measure(measurable, constraints.loosen());
                    scope.place(&placeable, 0, height);
                    width = width.max(placeable.width);
                    height += placeable.height;
                }
                scope.layout(width, height)
            }),
            callback(move || {
                at_least_size(50, cached.clone());
                at_least_size(0, clicked.clone());
            }),
        );
    });
    ui.wait_for_idle();
    assert_eq!(builds.get(), 1);
    let image = ui.on_node_with_tag("boxTag").capture_to_image();
    image.assert_pixels(Some(IntSize::new(50, 50)), |_| Some(Color::BLUE));
    ui.on_node_with_tag("clickTag").perform_click();
    ui.wait_for_idle();
    assert_eq!(builds.get(), 2);
    let image = ui.on_node_with_tag("boxTag").capture_to_image();
    image.assert_pixels(Some(IntSize::new(50, 50)), |_| Some(Color::RED));
}
