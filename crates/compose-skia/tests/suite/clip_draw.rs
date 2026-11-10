//! Compose's `ClipDrawTest` (ui, device tests): `clip` and `clip_to_bounds` to rectangles,
//! rounded rectangles, triangles and a concave path, shapes switched by state (as the clip's
//! shape, or read by the shape as it makes its outline), a clip emitted later, the layout
//! direction the shape gets, and a resized clip, drawn by Skia and checked pixel by pixel.
//! `androidView_visibilityGone` is n/a (an Android view).

use std::rc::Rc;

use compose::runtime::*;
use compose::test::*;
use compose::ui::{
    Color, Constraints, CustomShape, Density, IntSize, LOCAL_LAYOUT_DIRECTION, Layout,
    LayoutDirection, Modifier, Outline, Paint, Path, PathFillType, Rect, Shape, Size,
    measure_policy,
};

type Make = dyn Fn(Size, LayoutDirection) -> Outline;

/// A shape making its outline with a closure, as the test's `object : Shape`s.
struct FnShape(Rc<Make>);

impl CustomShape for FnShape {
    fn create_outline(&self, size: Size, direction: LayoutDirection, _: &Density) -> Outline {
        (self.0)(size, direction)
    }

    fn same(&self, other: &dyn CustomShape) -> bool {
        (other as &dyn std::any::Any)
            .downcast_ref::<FnShape>()
            .is_some_and(|other| Rc::ptr_eq(&self.0, &other.0))
    }
}

fn shape(make: impl Fn(Size, LayoutDirection) -> Outline + 'static) -> Shape {
    Shape::custom(FnShape(Rc::new(make)))
}

fn whole(size: Size) -> Rect {
    Rect::new(0.0, 0.0, size.width, size.height)
}

fn rect_shape() -> Shape {
    shape(|size, _| Outline::Rectangle(whole(size)))
}

/// Its top's middle, its bottom's corners.
fn triangle(path: &Path, size: Size) {
    path.move_to(size.width / 2.0, 0.0);
    path.line_to(size.width, size.height);
    path.line_to(0.0, size.height);
    path.close();
}

/// Its top's corners, its bottom's middle.
fn inverted_triangle(path: &Path, size: Size) {
    path.line_to(size.width, 0.0);
    path.line_to(size.width / 2.0, size.height);
    path.line_to(0.0, 0.0);
    path.close();
}

fn triangle_outline(size: Size) -> Outline {
    let path = Path::new();
    triangle(&path, size);
    Outline::generic(&path)
}

fn inverted_triangle_outline(size: Size) -> Outline {
    let path = Path::new();
    inverted_triangle(&path, size);
    Outline::generic(&path)
}

fn triangle_shape() -> Shape {
    shape(|size, _| triangle_outline(size))
}

fn rounded(radius: f32) -> Shape {
    shape(move |size, _| Outline::Rounded(whole(size), [radius; 4]))
}

/// Fills well beyond its layout with `color`, as the test's `fillColor`.
fn fill_color(modifier: Modifier, color: Color) -> Modifier {
    modifier.draw_behind(move |scope| {
        let size = scope.size();
        let rect = Rect::new(-100.0, -100.0, size.width + 100.0, size.height + 100.0);
        scope.draw_rect(rect, Paint::fill(color));
    })
}

/// Its content padded by `size` pixels, as the test utilities' `Padding`.
fn padding(size: i32, modifier: Modifier, content: impl Fn() + 'static) {
    Layout(
        modifier,
        measure_policy(move |scope, measurables, constraints| {
            let inner = constraints.offset(-size * 2, -size * 2);
            let (mut width, mut height) = (size, size);
            for &measurable in measurables {
                let placeable = scope.measure(measurable, inner);
                scope.place_relative(&placeable, size, size);
                width = width.max(placeable.width + size * 2);
                height = height.max(placeable.height + size * 2);
            }
            scope.layout(width, height)
        }),
        callback(content),
    );
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

/// The content inset by `padding` pixels, as the test utilities' `Modifier.padding(Int)`.
fn pad(modifier: Modifier, padding: i32) -> Modifier {
    modifier.layout(move |scope, measurable, constraints| {
        let placeable = scope.measure(measurable, constraints.offset(-padding * 2, -padding * 2));
        scope.place_relative(&placeable, padding, padding);
        scope.layout(
            constraints.constrain_width(placeable.width + padding * 2),
            constraints.constrain_height(placeable.height + padding * 2),
        )
    })
}

fn rule(content: impl Fn() + 'static) -> ComposeTestRule {
    set_capture_renderer(compose_skia::raster_colors);
    ComposeTestRule::sized(IntSize::new(30, 30), content)
}

/// The 30 × 30 window, as the test's `takeScreenShot(30)`.
fn take_screenshot(ui: &mut ComposeTestRule) -> CapturedImage {
    ui.wait_for_idle();
    let image = ui.on_root().capture_to_image();
    assert_eq!(image.size(), IntSize::new(30, 30));
    image
}

/// The square of `size` around the middle is `color`, but for a `hole` in its middle, as the
/// test utilities' `assertRect` (which checks only the pixels beyond the hole on both axes).
#[track_caller]
fn assert_rect(image: &CapturedImage, color: Color, size: i32, hole: i32) {
    assert_rect_at(image, color, size, hole, image.width() / 2, image.height() / 2);
}

#[track_caller]
fn assert_rect_at(image: &CapturedImage, color: Color, size: i32, hole: i32, cx: i32, cy: i32) {
    let half_hole = hole / 2;
    for x in cx - size / 2..cx + size / 2 {
        for y in cy - size / 2..cy + size / 2 {
            if (x - cx).abs() > half_hole && (y - cy).abs() > half_hole {
                image.assert_pixel_color(color, x, y);
            }
        }
    }
}

/// The test's `assertTriangle`.
#[track_caller]
fn assert_triangle(image: &CapturedImage, inner: Color, outer: Color) {
    let center = (image.width() - 1) / 2;
    let last = image.width() - 1;
    image.assert_pixel_color(inner, center, center);
    image.assert_pixel_color(outer, 4, 4);
    image.assert_pixel_color(outer, last - 4, 4);
    image.assert_pixel_color(outer, 0, last - 4);
    image.assert_pixel_color(inner, 4, last - 4);
    image.assert_pixel_color(outer, last, last - 6);
    image.assert_pixel_color(inner, last - 6, last);
    image.assert_pixel_color(outer, center - 4, 0);
    image.assert_pixel_color(outer, center + 4, 0);
    image.assert_pixel_color(inner, center, 8);
}

/// The test's `assertInvertedTriangle`.
#[track_caller]
fn assert_inverted_triangle(image: &CapturedImage, inner: Color, outer: Color) {
    let center = (image.width() - 1) / 2;
    let last = image.width() - 1;
    image.assert_pixel_color(inner, center, center);
    image.assert_pixel_color(outer, 0, 4);
    image.assert_pixel_color(inner, 4, 4);
    image.assert_pixel_color(outer, last, 4);
    image.assert_pixel_color(inner, last - 4, 0);
    image.assert_pixel_color(outer, 4, last - 4);
    image.assert_pixel_color(outer, last - 4, last - 4);
    image.assert_pixel_color(outer, center - 4, last);
    image.assert_pixel_color(outer, center + 4, last);
    image.assert_pixel_color(inner, center, last - 6);
}

/// Green around a corner of 12 at each of the four corners (Compose checks three; the fourth,
/// (27, 27), is green as well).
#[track_caller]
fn assert_green_corners(image: &CapturedImage) {
    image.assert_pixel_color(Color::GREEN, 2, 2);
    image.assert_pixel_color(Color::GREEN, 2, 27);
    image.assert_pixel_color(Color::GREEN, 27, 2);
    image.assert_pixel_color(Color::GREEN, 27, 27);
}

/// Green filling the 30 × 30 layout, clipped to `clip`, then cyan.
fn clipped(clip: Shape) -> Modifier {
    fill_color(fill_color(Modifier, Color::GREEN).clip(clip), Color::CYAN)
}

/// `clipped`'s fills, made once, outside the content: a test switching the clip makes only the
/// clip again in its compositions, so what draws the change is the clip's update, not a fill
/// made anew.
fn clipped_by(fills: &(Modifier, Modifier), clip: Shape) -> Modifier {
    fills.0.clone().clip(clip).then(fills.1.clone())
}

fn fills() -> (Modifier, Modifier) {
    (fill_color(Modifier, Color::GREEN), fill_color(Modifier, Color::CYAN))
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/ClipDrawTest.kt, simpleRectClip
#[test]
fn simple_rect_clip() {
    let mut ui = rule(|| {
        padding(10, fill_color(Modifier, Color::GREEN), || {
            at_least_size(10, fill_color(Modifier.clip(rect_shape()), Color::CYAN));
        });
    });
    let image = take_screenshot(&mut ui);
    assert_rect(&image, Color::CYAN, 10, 0);
    assert_rect(&image, Color::GREEN, 30, 10);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/ClipDrawTest.kt, simpleClipToBounds
#[test]
fn simple_clip_to_bounds() {
    let mut ui = rule(|| {
        padding(10, fill_color(Modifier, Color::GREEN), || {
            at_least_size(10, fill_color(Modifier.clip_to_bounds(), Color::CYAN));
        });
    });
    let image = take_screenshot(&mut ui);
    assert_rect(&image, Color::CYAN, 10, 0);
    assert_rect(&image, Color::GREEN, 30, 10);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/ClipDrawTest.kt, simpleRectClipWithModifiers
#[test]
fn simple_rect_clip_with_modifiers() {
    let mut ui = rule(|| {
        let modifier = pad(fill_color(Modifier, Color::GREEN), 10).clip(rect_shape());
        at_least_size(10, fill_color(modifier, Color::CYAN));
    });
    let image = take_screenshot(&mut ui);
    assert_rect(&image, Color::CYAN, 10, 0);
    assert_rect(&image, Color::GREEN, 30, 10);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/ClipDrawTest.kt, roundedUniformRectClip
#[test]
fn rounded_uniform_rect_clip() {
    let mut ui = rule(|| at_least_size(30, clipped(rounded(12.0))));
    let image = take_screenshot(&mut ui);
    assert_green_corners(&image);
    assert_rect(&image, Color::CYAN, 18, 0);
    for (x, y) in [(0, 14), (29, 14), (14, 0), (14, 29)] {
        image.assert_pixel_color(Color::CYAN, x, y);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/ClipDrawTest.kt, roundedRectWithDiffCornersClip
#[test]
fn rounded_rect_with_diff_corners_clip() {
    // Top left square, the others rounded by 12.
    let corners = shape(|size, _| Outline::Rounded(whole(size), [0.0, 12.0, 12.0, 12.0]));
    let mut ui = rule(move || at_least_size(30, clipped(corners.clone())));
    let image = take_screenshot(&mut ui);
    image.assert_pixel_color(Color::CYAN, 2, 2);
    image.assert_pixel_color(Color::GREEN, 2, 27);
    image.assert_pixel_color(Color::GREEN, 27, 2);
    // Beyond Compose: the fourth corner, rounded too.
    image.assert_pixel_color(Color::GREEN, 27, 27);
    assert_rect(&image, Color::CYAN, 18, 0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/ClipDrawTest.kt, triangleClip
#[test]
fn triangle_clip() {
    let mut ui = rule(|| at_least_size(30, clipped(triangle_shape())));
    assert_triangle(&take_screenshot(&mut ui), Color::CYAN, Color::GREEN);
}

/// A 30 pixel square with a 10 pixel square hole in its middle.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/ClipDrawTest.kt, concaveClip
#[test]
fn concave_clip() {
    let concave = shape(|_, _| {
        let path = Path::new();
        path.set_fill_type(PathFillType::EvenOdd);
        path.add_rect(Rect::new(0.0, 0.0, 30.0, 30.0));
        path.add_rect(Rect::new(10.0, 10.0, 20.0, 20.0));
        Outline::generic(&path)
    });
    let mut ui = rule(move || at_least_size(30, clipped(concave.clone())));
    let image = take_screenshot(&mut ui);
    assert_rect(&image, Color::GREEN, 10, 0);
    assert_rect(&image, Color::CYAN, 30, 10);
}

/// A clip whose shape is the state's, read in the composition.
fn switching(start: Shape) -> (ComposeTestRule, OwnedState<Shape>) {
    let model = mutable_state_of(start);
    let (m, fills) = (model.handle(), fills());
    let ui = rule(move || at_least_size(30, clipped_by(&fills, m.get())));
    (ui, model)
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/ClipDrawTest.kt, switchFromRectToRounded
#[test]
fn switch_from_rect_to_rounded() {
    let (mut ui, model) = switching(rect_shape());
    assert_rect(&take_screenshot(&mut ui), Color::CYAN, 30, 0);
    model.set(rounded(12.0));
    let image = take_screenshot(&mut ui);
    assert_green_corners(&image);
    // Beyond Compose's corners: the inside is still cyan.
    assert_rect(&image, Color::CYAN, 18, 0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/ClipDrawTest.kt, switchFromRectToPath
#[test]
fn switch_from_rect_to_path() {
    let (mut ui, model) = switching(rect_shape());
    assert_rect(&take_screenshot(&mut ui), Color::CYAN, 30, 0);
    model.set(triangle_shape());
    assert_triangle(&take_screenshot(&mut ui), Color::CYAN, Color::GREEN);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/ClipDrawTest.kt, switchFromPathToRect
#[test]
fn switch_from_path_to_rect() {
    let (mut ui, model) = switching(triangle_shape());
    assert_triangle(&take_screenshot(&mut ui), Color::CYAN, Color::GREEN);
    model.set(rect_shape());
    assert_rect(&take_screenshot(&mut ui), Color::CYAN, 30, 0);
}

/// Green, then a layer clipping to `shape()` (read in its block), then cyan.
fn layer_clipped(shape: impl Fn() -> Shape + 'static) -> Modifier {
    let clip = Modifier.graphics_layer(move |layer| {
        layer.shape = shape();
        layer.clip = true;
    });
    fill_color(Modifier.background(Color::GREEN).then(clip), Color::CYAN)
}

/// The shape switched in the layer's block, the content not composed again.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/ClipDrawTest.kt, switchBetweenDifferentPaths
#[test]
fn switch_between_different_paths() {
    let inverted_shape = shape(|size, _| inverted_triangle_outline(size));
    let shapes = [triangle_shape(), inverted_shape];
    let which = mutable_state_of(0usize);
    let w = which.handle();
    let modifier = layer_clipped(move || shapes[w.get()].clone());
    let mut ui = rule(move || at_least_size(30, modifier.clone()));
    assert_triangle(&take_screenshot(&mut ui), Color::CYAN, Color::GREEN);
    which.set(1);
    assert_inverted_triangle(&take_screenshot(&mut ui), Color::CYAN, Color::GREEN);
}

/// One shape reading a state as it makes its outline: the outline is made again when it changes.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/ClipDrawTest.kt, switchBetweenDifferentOutlines_differentPath_observableShape
#[test]
fn switch_between_different_outlines_different_path_observable_shape() {
    let inverted = mutable_state_of(false);
    let i = inverted.handle();
    let observable =
        shape(
            move |size, _| {
                if i.get() { inverted_triangle_outline(size) } else { triangle_outline(size) }
            },
        );
    let modifier = layer_clipped(move || observable.clone());
    let mut ui = rule(move || at_least_size(30, modifier.clone()));
    assert_triangle(&take_screenshot(&mut ui), Color::CYAN, Color::GREEN);
    inverted.set(true);
    assert_inverted_triangle(&take_screenshot(&mut ui), Color::CYAN, Color::GREEN);
}

/// The same, the outline made of one path reset and drawn again each time (Compose's
/// `addOutline` into it): compose-rs's outline holds the path's contents (D1925).
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/ClipDrawTest.kt, switchBetweenDifferentOutlines_samePath_observableShape
#[test]
fn switch_between_different_outlines_same_path_observable_shape() {
    let inverted = mutable_state_of(false);
    let i = inverted.handle();
    let path = Path::new();
    let observable = shape(move |size, _| {
        path.reset();
        if i.get() {
            inverted_triangle(&path, size)
        } else {
            triangle(&path, size)
        }
        Outline::generic(&path)
    });
    let modifier = layer_clipped(move || observable.clone());
    let mut ui = rule(move || at_least_size(30, modifier.clone()));
    assert_triangle(&take_screenshot(&mut ui), Color::CYAN, Color::GREEN);
    inverted.set(true);
    assert_inverted_triangle(&take_screenshot(&mut ui), Color::CYAN, Color::GREEN);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/ClipDrawTest.kt, emitClipLater
#[test]
fn emit_clip_later() {
    let model = mutable_state_of(false);
    let m = model.handle();
    let mut ui = rule(move || {
        padding(10, fill_color(Modifier, Color::GREEN), move || {
            let modifier = if m.get() {
                fill_color(Modifier.clip(rect_shape()), Color::CYAN)
            } else {
                Modifier
            };
            at_least_size(10, modifier);
        });
    });
    ui.wait_for_idle();
    // Beyond Compose: before the clip, all green.
    assert_rect(&take_screenshot(&mut ui), Color::GREEN, 30, 0);
    model.set(true);
    let image = take_screenshot(&mut ui);
    assert_rect(&image, Color::CYAN, 10, 0);
    assert_rect(&image, Color::GREEN, 30, 10);
}

/// The shape and the fills made once, so the layout direction alone draws the change.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/ClipDrawTest.kt, switchLayoutDirection
#[test]
fn switch_layout_direction() {
    let direction = mutable_state_of(LayoutDirection::Ltr);
    let d = direction.handle();
    let by_direction = shape(|size, direction| match direction {
        LayoutDirection::Ltr => Outline::Rectangle(whole(size)),
        LayoutDirection::Rtl => triangle_outline(size),
    });
    let modifier = clipped_by(&fills(), by_direction);
    let mut ui = rule(move || {
        let modifier = modifier.clone();
        CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(d.get()), move || {
            at_least_size(30, modifier.clone());
        });
    });
    assert_rect(&take_screenshot(&mut ui), Color::CYAN, 30, 0);
    direction.set(LayoutDirection::Rtl);
    assert_triangle(&take_screenshot(&mut ui), Color::CYAN, Color::GREEN);
}

/// The modifier made once, so the size read in its layout alone lays out and draws the change.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/ClipDrawTest.kt, resizingIsReflectedInGraphicsLayer
#[test]
fn resizing_is_reflected_in_graphics_layer() {
    let size = mutable_state_of(20);
    let s = size.handle();
    let modifier = Modifier
        .background(Color::WHITE)
        .layout(|scope, measurable, constraints| {
            let placeable = scope.measure(measurable, constraints);
            scope.place(&placeable, 0, 0);
            scope.layout(30, 30)
        })
        .layout(move |scope, measurable, constraints| {
            let px = s.get();
            let placeable = scope.measure(measurable, Constraints::fixed(px, px));
            scope.place(&placeable, 0, 0);
            scope.layout(constraints.constrain_width(px), constraints.constrain_height(px))
        })
        .clip_to_bounds();
    let modifier = fill_color(modifier, Color::RED);
    let mut ui = rule(move || {
        // Compose's `Box` with no content: its minimum size.
        Layout(
            modifier.clone(),
            measure_policy(|scope, _, constraints| {
                scope.layout(constraints.min_width, constraints.min_height)
            }),
            callback(|| {}),
        );
    });
    let image = take_screenshot(&mut ui);
    assert_rect_at(&image, Color::RED, 20, 0, 10, 10);
    // Beyond Compose: the rest is the white background, the red clipped to 20 × 20.
    for (x, y) in [(20, 0), (0, 20), (20, 20), (29, 29)] {
        image.assert_pixel_color(Color::WHITE, x, y);
    }
    size.set(30);
    assert_rect(&take_screenshot(&mut ui), Color::RED, 30, 0);
}
