//! What one fill can touch (`single_drawing_extent`, D648): an alpha layer folds into that
//! fill's paint only when its bounds hold it, so the extent must never be smaller than what the
//! fill touches, and is `None` where that can't be bounded, or where the fold wouldn't draw the
//! same pixels (a stroke).

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

/// A rect, a rounded rect or an oval given right before left (or bottom before top) is drawn
/// sorted, as Skia's and Android's: its extent too.
#[test]
fn an_inverted_rect_is_sorted() {
    let inverted = Rect::new(30.0, 5.0, 10.0, 15.0);
    let sorted = Rect::new(10.0, 5.0, 30.0, 15.0);
    let fill = Paint::fill(Color::RED);
    assert_eq!(extent(vec![DrawOp::DrawRect(inverted, fill)]), Some(sorted));
    assert_eq!(extent(vec![DrawOp::DrawRoundRect(inverted, [2.0; 4], fill)]), Some(sorted));
    let arc = DrawOp::DrawArc {
        oval: inverted,
        start_angle: 0.0,
        sweep_angle: 90.0,
        use_center: true,
        paint: fill,
    };
    assert_eq!(extent(vec![arc]), Some(sorted));
    assert!(!fits_in(sorted, Rect::new(0.0, 0.0, 20.0, 20.0)));
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

/// A stroke, a hairline (zero width) or a line (always a stroke, a hairline with a fill paint)
/// isn't drawn the same faded and offscreen: thin, its joints are blended twice; wide, a
/// rounded rect's corners differ (D648). Never bounded, never folded.
#[test]
fn a_stroke_has_no_extent() {
    for width in [0.0, 0.5, 1.0, 2.0, 4.0] {
        let paint = Paint::stroke(Color::RED, width);
        assert_eq!(extent(vec![DrawOp::DrawRect(Rect::new(0.0, 0.0, 4.0, 4.0), paint)]), None);
        let round = DrawOp::DrawRoundRect(Rect::new(0.0, 0.0, 4.0, 4.0), [1.0; 4], paint);
        assert_eq!(extent(vec![round]), None);
        let arc = DrawOp::DrawArc {
            oval: Rect::new(0.0, 0.0, 10.0, 10.0),
            start_angle: 0.0,
            sweep_angle: 30.0,
            use_center: false,
            paint,
        };
        assert_eq!(extent(vec![arc]), None);
        let path = Path::new();
        path.move_to(0.0, 0.0);
        path.line_to(10.0, 10.0);
        let path =
            DrawOp::DrawPath { path: path.data(), offset: Offset::ZERO, scale: (1.0, 1.0), paint };
        assert_eq!(extent(vec![path]), None);
    }
    for paint in [Paint::fill(Color::RED), Paint::stroke(Color::RED, 4.0)] {
        let line = DrawOp::DrawLine(Offset::new(0.0, 0.0), Offset::new(4.0, 4.0), paint);
        assert_eq!(extent(vec![line]), None);
    }
}

#[test]
fn a_transform_inside_or_two_drawings_have_no_extent() {
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
    let rect = DrawOp::DrawRect(Rect::new(0.0, 0.0, 4.0, 4.0), Paint::fill(Color::RED));
    assert_eq!(extent(vec![rect.clone(), rect]), None);
}

/// Text, images and shadows draw what a paint's alpha doesn't reach: never one drawing to fold.
#[test]
fn text_images_and_shadows_have_no_extent() {
    let paragraph = DrawOp::DrawParagraph(ParagraphHandle(0), Offset::ZERO);
    assert_eq!(extent(vec![paragraph]), None);
    let image = DrawOp::DrawImage {
        image: ImageBitmap::from_rgba_premultiplied(1, 1, vec![0; 4]),
        src: Rect::new(0.0, 0.0, 1.0, 1.0),
        dst: Rect::new(0.0, 0.0, 4.0, 4.0),
        alpha: 1.0,
        tint: None,
    };
    assert_eq!(extent(vec![image]), None);
    let shadow = DrawOp::DrawShadow {
        outline: Outline::Rectangle(Rect::new(0.0, 0.0, 4.0, 4.0)),
        elevation: 4.0,
        light: (0.0, 0.0, 600.0),
        light_radius: 800.0,
        ambient: Color::BLACK,
        spot: Color::BLACK,
        transparent_occluder: false,
    };
    assert_eq!(extent(vec![shadow]), None);
}

#[test]
fn an_extent_fits_bounds_that_hold_it() {
    let bounds = Rect::new(0.0, 0.0, 20.0, 20.0);
    assert!(fits_in(Rect::new(0.0, 0.0, 20.0, 20.0), bounds));
    assert!(!fits_in(Rect::new(0.5, 0.0, 20.5, 20.0), bounds));
    assert!(!fits_in(Rect::new(-0.5, 0.0, 10.0, 10.0), bounds));
    // Inverted: it holds nothing, nor fits anywhere.
    assert!(!fits_in(Rect::new(15.0, 5.0, 10.0, 15.0), bounds));
    assert!(!fits_in(Rect::new(5.0, 15.0, 10.0, 10.0), bounds));
    assert!(!fits_in(Rect::new(5.0, 5.0, 10.0, 10.0), Rect::new(20.0, 20.0, 0.0, 0.0)));
}
