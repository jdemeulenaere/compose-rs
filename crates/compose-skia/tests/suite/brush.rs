//! Gradients and path clips rendered by Skia: a paint's shader comes from its display list,
//! in the list's coordinates (a scaled path's included).

use std::sync::Arc;

use compose::ui::{
    Brush, Color, Offset, Paint, Path, Rect, Size, TileMode,
    platform::{DisplayList, DrawOp, FixedTextMeasurer, Renderer},
};
use compose_skia::RasterRenderer;

fn render(list: &DisplayList) -> RasterRenderer {
    let mut renderer = RasterRenderer::new(100, 100);
    renderer.render(list, Color::WHITE, 1.0, &FixedTextMeasurer::default());
    renderer
}

fn gradient_paint(list: &mut DisplayList, brush: &Brush, size: Size, origin: Offset) -> Paint {
    let paint = Paint::fill(brush.color(1.0));
    match brush.shader(size, origin, 1.0) {
        Some(shader) => paint.with_shader(list.push_shader(shader)),
        None => paint,
    }
}

#[test]
fn a_horizontal_gradient_goes_from_its_first_color_to_its_last() {
    let mut list = DisplayList::default();
    let brush = Brush::horizontal_gradient_with(
        [Color::RED, Color::BLUE],
        0.0,
        f32::INFINITY,
        TileMode::Clamp,
    );
    let paint = gradient_paint(&mut list, &brush, Size::new(100.0, 100.0), Offset::ZERO);
    list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 100.0, 100.0), paint));
    let mut renderer = render(&list);
    let left = renderer.pixel(1, 50);
    let right = renderer.pixel(98, 50);
    assert!(left.red() > 240 && left.blue() < 15, "{left:?}");
    assert!(right.blue() > 240 && right.red() < 15, "{right:?}");
    let middle = renderer.pixel(50, 50);
    assert!((100..160).contains(&middle.red()), "{middle:?}");
}

#[test]
fn a_radial_gradient_and_a_shaded_path_stay_in_the_lists_coordinates() {
    let mut list = DisplayList::default();
    let brush = Brush::radial_gradient_with(
        [Color::BLACK, Color::WHITE],
        Offset::UNSPECIFIED,
        f32::INFINITY,
        TileMode::Clamp,
    );
    let paint = gradient_paint(&mut list, &brush, Size::new(100.0, 100.0), Offset::ZERO);
    // A unit square scaled to the whole surface: the gradient isn't scaled with it.
    let mut path = Path::new();
    path.move_to(0.0, 0.0);
    path.line_to(1.0, 0.0);
    path.line_to(1.0, 1.0);
    path.line_to(0.0, 1.0);
    path.close();
    list.push(DrawOp::DrawPath {
        path: Arc::new(path),
        offset: Offset::ZERO,
        scale: (100.0, 100.0),
        paint,
    });
    let mut renderer = render(&list);
    let center = renderer.pixel(50, 50);
    let edge = renderer.pixel(99, 50);
    assert!(center.red() < 20, "{center:?}");
    assert!(edge.red() > 230, "{edge:?}");
}

#[test]
fn a_path_clip_keeps_what_is_inside_it() {
    let mut list = DisplayList::default();
    let mut triangle = Path::new();
    triangle.move_to(0.0, 0.0);
    triangle.line_to(100.0, 0.0);
    triangle.line_to(0.0, 100.0);
    triangle.close();
    list.push(DrawOp::Save);
    list.push(DrawOp::ClipPath { path: Arc::new(triangle), offset: Offset::ZERO });
    list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 100.0, 100.0), Paint::fill(Color::RED)));
    list.push(DrawOp::Restore);
    let mut renderer = render(&list);
    assert_eq!(renderer.pixel(10, 10), Color::RED);
    assert_eq!(renderer.pixel(90, 90), Color::WHITE);
}

/// A radial gradient without a radius is a solid color: its last color when clamped, nothing
/// with decal (what Android's serializer writes instead of a gradient, D210).
#[test]
fn a_radial_gradient_without_a_radius_is_solid() {
    let draw = |tile_mode: TileMode| {
        let mut list = DisplayList::default();
        let brush =
            Brush::radial_gradient_with([Color::RED, Color::BLUE], Offset::ZERO, 0.0, tile_mode);
        let paint = gradient_paint(&mut list, &brush, Size::new(100.0, 100.0), Offset::ZERO);
        list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 100.0, 100.0), paint));
        render(&list).pixel(50, 50)
    };
    assert_eq!(draw(TileMode::Clamp), Color::BLUE);
    assert_eq!(draw(TileMode::Decal), Color::WHITE);
}

/// Colors are interpolated premultiplied, as Android's gradients (HWUI's
/// `kInterpolateColorsInPremul_Flag`) and Skiko's default: halfway from transparent to red is
/// red at half alpha (pink over white), not a darker red at half alpha.
#[test]
fn a_gradient_interpolates_premultiplied_colors() {
    let mut list = DisplayList::default();
    let brush = Brush::horizontal_gradient_with(
        [Color::TRANSPARENT, Color::RED],
        0.0,
        f32::INFINITY,
        TileMode::Clamp,
    );
    let paint = gradient_paint(&mut list, &brush, Size::new(100.0, 100.0), Offset::ZERO);
    list.push(DrawOp::DrawRect(Rect::new(0.0, 0.0, 100.0, 100.0), paint));
    let mut renderer = render(&list);
    // Half red over white: (255, 128, 128); unpremultiplied would give (191, 128, 128).
    let middle = renderer.pixel(50, 50);
    assert!(middle.red() > 245, "{middle:?}");
    assert!((115..140).contains(&middle.green()), "{middle:?}");
}
