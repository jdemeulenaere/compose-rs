//! Gradients and path clips rendered by Skia: a paint's shader comes from its display list,
//! in the list's coordinates (a scaled path's included).

use std::sync::Arc;

use compose::ui::{
    Brush, Color, DisplayList, DrawOp, FixedTextMeasurer, Offset, Paint, Path, Rect, Renderer,
    Size, TileMode,
};
use compose_skia::RasterRenderer;

fn render(list: &DisplayList) -> RasterRenderer {
    let mut renderer = RasterRenderer::new(100, 100);
    renderer.render(list, Color::WHITE, 1.0, &FixedTextMeasurer::default());
    renderer
}

fn gradient_paint(list: &mut DisplayList, brush: &Brush, size: Size, origin: Offset) -> Paint {
    let mut paint = Paint::fill(brush.color(1.0));
    paint.shader = brush.shader(size, origin, 1.0).map(|shader| list.push_shader(shader));
    paint
}

#[test]
fn a_horizontal_gradient_goes_from_its_first_color_to_its_last() {
    let mut list = DisplayList::default();
    let brush =
        Brush::horizontal_gradient(&[Color::RED, Color::BLUE], 0.0, f32::INFINITY, TileMode::Clamp);
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
    let brush = Brush::radial_gradient(
        &[Color::BLACK, Color::WHITE],
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
