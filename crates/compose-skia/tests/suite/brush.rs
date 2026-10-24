//! Gradients and path clips rendered by Skia: a paint's shader comes from its display list,
//! in the list's coordinates (a scaled path's included).

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
    let path = Path::new();
    path.move_to(0.0, 0.0);
    path.line_to(1.0, 0.0);
    path.line_to(1.0, 1.0);
    path.line_to(0.0, 1.0);
    path.close();
    list.push(DrawOp::DrawPath {
        path: path.data(),
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
    let triangle = Path::new();
    triangle.move_to(0.0, 0.0);
    triangle.line_to(100.0, 0.0);
    triangle.line_to(0.0, 100.0);
    triangle.close();
    list.push(DrawOp::Save);
    list.push(DrawOp::ClipPath { path: triangle.data(), offset: Offset::ZERO });
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

/// A remembered `Path` reset and rebuilt in its drawing block each frame (D530) renders as it
/// is in each frame: Skia's path for it is made again for each change (D532), not the one made
/// for its first contents.
#[test]
fn a_path_reset_each_frame_renders_as_it_is() {
    use compose::foundation::layout::Canvas;
    use compose::runtime::{MutableState, mutable_state_of, remember};
    use compose::ui::{Density, IntSize, Modifier, UnitExt, platform::UiHost};
    thread_local! {
        static WIDTH: std::cell::Cell<Option<MutableState<f32>>> = const { std::cell::Cell::new(None) };
    }
    let width = mutable_state_of(30.0f32);
    WIDTH.set(Some(width.handle()));
    fn content() {
        let path = remember(Path::new);
        let width = WIDTH.get().expect("a width");
        Canvas(Modifier.size(100.dp()), move |scope| {
            path.reset();
            path.add_rect(Rect::new(0.0, 0.0, width.get(), 100.0));
            scope.draw_path(&path, Paint::fill(Color::RED));
        });
    }
    let mut host = UiHost::new(
        IntSize::new(100, 100),
        Density::new(1.0),
        Box::new(FixedTextMeasurer::default()),
    );
    host.set_content(content);
    for (wide, inside, outside) in [(30.0, 25, 35), (80.0, 75, 85), (10.0, 5, 15)] {
        width.set(wide);
        host.frame();
        let mut renderer = RasterRenderer::new(100, 100);
        host.with_tree(|tree| {
            renderer.render(tree.display_list(), Color::WHITE, 1.0, tree.text_measurer_ref())
        });
        assert_eq!(renderer.pixel(inside, 50), Color::RED, "width {wide}");
        assert_eq!(renderer.pixel(outside, 50), Color::WHITE, "width {wide}");
    }
}

/// `Path::op` is drawn as Skia's path ops compute it: a square less a smaller one is a frame.
#[test]
fn a_path_operation_renders_its_result() {
    let (outer, inner) = (Path::new(), Path::new());
    outer.add_rect(Rect::new(0.0, 0.0, 100.0, 100.0));
    inner.add_rect(Rect::new(25.0, 25.0, 75.0, 75.0));
    let frame = Path::combine(compose::ui::PathOperation::Difference, &outer, &inner);
    let mut list = DisplayList::default();
    list.push(DrawOp::DrawPath {
        path: frame.data(),
        offset: Offset::ZERO,
        scale: (1.0, 1.0),
        paint: Paint::fill(Color::RED),
    });
    let mut renderer = render(&list);
    assert_eq!(renderer.pixel(10, 10), Color::RED);
    assert_eq!(renderer.pixel(50, 50), Color::WHITE, "the hole");
}

/// On desktop `Path::op` is computed by Skia when it's called (D535), as Compose Desktop's: the
/// result is flat contents, exact for `is_empty`; an operation Skia can't do leaves the path as it
/// was and returns `false`, as Android's.
#[test]
fn desktop_computes_path_operations_when_called() {
    use compose::ui::{PathCommand, PathOperation};
    compose_skia::install_path_operator();
    let (outer, inner) = (Path::new(), Path::new());
    outer.add_rect(Rect::new(0.0, 0.0, 100.0, 100.0));
    inner.add_rect(Rect::new(25.0, 25.0, 75.0, 75.0));
    let frame = Path::combine(PathOperation::Difference, &outer, &inner);
    let data = frame.data();
    assert!(data.steps().is_empty(), "computed: {:?}", data.steps());
    let closes = data.commands().iter().filter(|command| matches!(command, PathCommand::Close));
    assert_eq!(closes.count(), 2);
    assert_eq!(frame.get_bounds(), Rect::new(0.0, 0.0, 100.0, 100.0));
    let nothing = Path::combine(PathOperation::Difference, &inner, &outer);
    assert!(nothing.is_empty(), "a path less one around it: exactly empty");
    let broken = Path::new();
    broken.move_to(f32::NAN, 0.0);
    broken.line_to(1.0, f32::INFINITY);
    broken.line_to(2.0, 2.0);
    let kept = Path::new();
    kept.add_rect(Rect::new(0.0, 0.0, 1.0, 1.0));
    let before = (kept.generation(), kept.data().commands().len());
    let done = kept.op(&kept, &broken, PathOperation::Union);
    if !done {
        assert_eq!((kept.generation(), kept.data().commands().len()), before, "unchanged");
    }
}
