//! What one drawing can touch (`single_drawing_extent`, D648): an alpha layer folds into that
//! drawing's paint only when its bounds hold it, so the extent must never be smaller than what
//! the drawing touches, and is `None` where that can't be bounded cheaply.

use std::sync::Arc;

use compose_ui::platform::*;
use compose_ui::*;

fn extent(ops: Vec<DrawOp>) -> Option<Rect> {
    single_drawing_extent(&ops)
}

#[test]
fn a_fill_is_its_geometry_moved() {
    let ops = vec![
        DrawOp::Save,
        DrawOp::Translate(Offset::new(2.0, 3.0)),
        DrawOp::ClipRect(Rect::new(0.0, 0.0, 1.0, 1.0)),
        DrawOp::DrawRect(Rect::new(1.0, 1.0, 5.0, 5.0), Paint::fill(Color::RED)),
        DrawOp::Restore,
    ];
    // A clip only narrows what it touches: the extent ignores it.
    assert_eq!(extent(ops), Some(Rect::new(3.0, 4.0, 7.0, 8.0)));
}

#[test]
fn a_stroke_grows_by_half_its_width_more_past_square_caps() {
    let line = |cap| {
        let paint = Paint::stroke_with(Color::RED, 4.0, cap);
        extent(vec![DrawOp::DrawLine(Offset::new(0.0, 0.0), Offset::new(10.0, 0.0), paint)])
            .expect("bounded")
    };
    assert_eq!(line(StrokeCap::Butt), Rect::new(-2.0, -2.0, 12.0, 2.0));
    assert_eq!(line(StrokeCap::Round), Rect::new(-2.0, -2.0, 12.0, 2.0));
    let square = line(StrokeCap::Square);
    assert!(square.left < -2.8 && square.right > 12.8, "{square:?}");
    // A hairline is a pixel wide.
    let hairline = Paint::stroke(Color::RED, 0.0);
    assert_eq!(
        extent(vec![DrawOp::DrawRect(Rect::new(0.0, 0.0, 4.0, 4.0), hairline)]),
        Some(Rect::new(-0.5, -0.5, 4.5, 4.5))
    );
}

#[test]
fn a_filled_path_is_its_control_points_scaled_and_moved() {
    let path = Path::new();
    path.move_to(0.0, 0.0);
    path.cubic_to(10.0, -5.0, 20.0, 5.0, 10.0, 10.0);
    let ops = vec![DrawOp::DrawPath {
        path: path.data(),
        offset: Offset::new(1.0, 1.0),
        scale: (2.0, 1.0),
        paint: Paint::fill(Color::RED),
    }];
    assert_eq!(extent(ops), Some(Rect::new(1.0, -4.0, 41.0, 11.0)));
}

#[test]
fn what_cant_be_bounded_cheaply_has_no_extent() {
    let path = Path::new();
    path.move_to(0.0, 0.0);
    path.line_to(10.0, 10.0);
    let path: Arc<_> = path.data();
    let stroke = Paint::stroke(Color::RED, 2.0);
    // A stroked path's miter joins.
    let ops =
        vec![DrawOp::DrawPath { path, offset: Offset::ZERO, scale: (1.0, 1.0), paint: stroke }];
    assert_eq!(extent(ops), None);
    // A stroked wedge's.
    let wedge = DrawOp::DrawArc {
        oval: Rect::new(0.0, 0.0, 10.0, 10.0),
        start_angle: 0.0,
        sweep_angle: 30.0,
        use_center: true,
        paint: stroke,
    };
    assert_eq!(extent(vec![wedge]), None);
    // A transform inside.
    let ops = vec![
        DrawOp::Save,
        DrawOp::Transform {
            pivot: Offset::ZERO,
            scale: (2.0, 2.0),
            rotation: 0.0,
            translation: Offset::ZERO,
        },
        DrawOp::DrawRect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::RED)),
        DrawOp::Restore,
    ];
    assert_eq!(extent(ops), None);
    // Two drawings.
    let rect = DrawOp::DrawRect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::RED));
    assert_eq!(extent(vec![rect.clone(), rect]), None);
}

#[test]
fn an_extent_fits_bounds_that_hold_it() {
    let bounds = Rect::new(0.0, 0.0, 20.0, 20.0);
    assert!(fits_in(Rect::new(0.0, 0.0, 20.0, 20.0), bounds));
    assert!(!fits_in(Rect::new(0.5, 0.0, 20.5, 20.0), bounds));
    assert!(!fits_in(Rect::new(-0.5, 0.0, 10.0, 10.0), bounds));
}
