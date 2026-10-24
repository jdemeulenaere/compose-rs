//! Custom layout and drawing ergonomics: a closure as a measure policy, `DrawScope`'s circles,
//! ovals, pie arcs, scoped transforms and clips, `draw_path` over a borrowed `Path`, paint
//! alpha, brushes (`brush_paint`, `background` with a brush) and `draw_with_cache`.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Brush, IntSize, Layout, Offset, Paint, PaintStyle, Path, Rect, TileMode, UnitExt,
    measure_policy,
};

#[test]
fn a_closure_is_a_measure_policy() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        // Its children side by side, as a row would.
        Layout!(
            modifier = Modifier.test_tag("row"),
            measure_policy = measure_policy(|scope, measurables, constraints| {
                let mut x = 0;
                let mut height = 0;
                for &measurable in measurables {
                    let placeable = scope.measure(measurable, constraints.loosen());
                    scope.place(&placeable, x, 0);
                    x += placeable.width;
                    height = height.max(placeable.height);
                }
                scope.layout(x, height)
            }),
            {
                Box!(modifier = Modifier.size(20.dp()));
                Box!(modifier = Modifier.size(30.dp()).test_tag("second"));
            }
        );
    });
    assert_eq!(ui.on_node_with_tag("second").fetch_semantics_node().bounds.left, 20.0);
    assert_eq!(ui.on_node_with_tag("row").fetch_semantics_node().bounds.width(), 50.0);
    assert!(ui.dump_tree().contains("Layout [0,0 50x30]"), "{}", ui.dump_tree());
}

#[test]
fn circles_ovals_and_pie_arcs_draw_as_arcs() {
    let ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        Canvas(Modifier.size(100.dp()), |scope| {
            let center = scope.center();
            scope.draw_circle(center, 10.0, Paint::fill(Color::RED));
            scope.draw_oval(Rect::new(0.0, 0.0, 40.0, 20.0), Paint::stroke(Color::BLUE, 2.0));
            scope.draw_arc_with(
                Rect::new(0.0, 0.0, 50.0, 50.0),
                0.0,
                90.0,
                true,
                Paint::fill(Color::GREEN).with_alpha(0.5),
            );
        });
    });
    let display = ui.dump_display_list();
    assert!(display.contains("arc 40,40 20x20 from 0 sweep 360 #FFFF0000"), "{display}");
    assert!(display.contains("arc 0,0 40x20 from 0 sweep 360 #FF0000FF stroke 2"), "{display}");
    assert!(display.contains("arc 0,0 50x50 from 0 sweep 90 to center #8000FF00"), "{display}");
}

#[test]
fn scoped_transforms_and_clips_restore_after_their_block() {
    let ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        Canvas(Modifier.size(100.dp()), |scope| {
            let triangle = Path::new();
            triangle.move_to(0.0, 0.0);
            triangle.line_to(10.0, 0.0);
            triangle.line_to(0.0, 10.0);
            triangle.close();
            scope.rotate(45.0, Offset::new(50.0, 50.0), |scope| {
                scope.draw_rect(Rect::new(0.0, 0.0, 10.0, 10.0), Paint::fill(Color::RED));
            });
            scope.scale(2.0, 3.0, Offset::ZERO, |scope| {
                scope.translate(5.0, 6.0, |scope| {
                    scope.draw_rect(Rect::new(0.0, 0.0, 1.0, 1.0), Paint::fill(Color::BLUE));
                });
            });
            scope.inset(10.0, 20.0, 30.0, 40.0, |scope| {
                let bounds = scope.bounds();
                scope.clip_rect(Rect::new(0.0, 0.0, 5.0, 5.0), |scope| {
                    scope.draw_rect(bounds, Paint::fill(Color::GREEN));
                });
            });
            scope.clip_path(&triangle, |scope| {
                scope.draw_path(&triangle, Paint::fill(Color::BLACK));
            });
        });
    });
    let display = ui.dump_display_list();
    let expected = "\
save
  transform pivot 50,50 scale 1x1 rotate 45 translate 0,0
  rect 0,0 10x10 #FFFF0000
restore
save
  transform pivot 0,0 scale 2x3 rotate 0 translate 0,0
  rect 5,6 1x1 #FF0000FF
restore
save
  clip 10,20 5x5
  rect 10,20 60x40 #FF00FF00
restore
save
  clip path of 4 commands at 0,0
  path of 4 commands at 0,0 scale 1x1 #FF000000
restore
";
    assert!(display.contains(expected), "{display}");
}

#[test]
fn a_brush_paints_a_gradient_in_the_drawings_coordinates() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), || {
        Column!({
            Box!(modifier = Modifier.size(10.dp()));
            Box!(
                modifier = Modifier.size((100.dp(), 50.dp())).background_with(
                    Brush::horizontal_gradient_with(
                        [Color::RED, Color::BLUE],
                        0.0,
                        f32::INFINITY,
                        TileMode::Clamp
                    ),
                    RECTANGLE_SHAPE
                )
            );
            Canvas(Modifier.size(40.dp()), |scope| {
                let brush = Brush::radial_gradient_with(
                    [(0.0, Color::WHITE), (0.5, Color::BLACK)],
                    Offset::UNSPECIFIED,
                    f32::INFINITY,
                    TileMode::Mirror,
                );
                let paint = scope.brush_paint(&brush, 0.5, PaintStyle::Stroke(2.0));
                let bounds = scope.bounds();
                scope.draw_rect(bounds, paint);
            });
        });
    });
    let display = ui.dump_display_list();
    // Its positions are the drawing's: from its left to its right edge (in the list's
    // coordinates, moved by the translations as the rects are).
    assert!(
        display.contains(
            "translate 0 10\nrect 0,0 100x50 linear 0,0 to 100,0 [#FFFF0000, #FF0000FF] Clamp"
        ),
        "{display}"
    );
    assert!(
        display.contains(
            "rect 0,0 40x40 radial at 20,20 radius 20 [#FFFFFFFF, #FF000000] at [0.0, 0.5] \
             Mirror alpha 0.5 stroke 2"
        ),
        "{display}"
    );
    // A renderer without shaders paints the average.
    let fallback = ui.host().display_list().ops.iter().find_map(|op| match op {
        compose_ui::platform::DrawOp::DrawRect(_, paint) if paint.shader().is_some() => {
            Some(paint.color)
        }
        _ => None,
    });
    assert_eq!(fallback, Some(Color::from_argb(0xFF80_0080)));
}

#[test]
fn gradients_in_repaint_boundaries_keep_their_shaders_when_spliced() {
    let shift = mutable_state_of(0.0f32);
    let handle = shift.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let gradient = |colors: [Color; 2]| {
            Brush::vertical_gradient_with(colors, 0.0, f32::INFINITY, TileMode::Clamp)
        };
        Column!(modifier = Modifier.offset(0.dp(), handle.get().dp()), {
            Box!(
                modifier = Modifier
                    .repaint_boundary()
                    .size(10.dp())
                    .background_with(gradient([Color::RED, Color::GREEN]), RECTANGLE_SHAPE)
            );
            Box!(
                modifier = Modifier
                    .repaint_boundary()
                    .size(10.dp())
                    .background_with(gradient([Color::BLUE, Color::BLACK]), RECTANGLE_SHAPE)
            );
        });
    });
    let check = |ui: &mut ComposeTestRule| {
        let list = ui.host().display_list();
        assert_eq!(list.shaders.len(), 2);
        let colors: Vec<Color> = list
            .ops
            .iter()
            .filter_map(|op| match op {
                compose_ui::platform::DrawOp::DrawRect(_, paint) => paint.shader(),
                _ => None,
            })
            .map(|index| list.shaders[index.0 as usize].gradient().colors[0])
            .collect();
        assert_eq!(colors, vec![Color::RED, Color::BLUE]);
    };
    check(&mut ui);
    // Moved: the boundaries' kept recordings are spliced again.
    shift.set(5.0);
    ui.wait_for_idle();
    check(&mut ui);
}

#[test]
fn draw_with_cache_builds_again_only_for_a_new_size_or_state_it_read() {
    let builds = Rc::new(Cell::new(0));
    let (width, color, tint) =
        (mutable_state_of(50.0f32), mutable_state_of(Color::RED), mutable_state_of(1.0f32));
    let (counted, width_handle, color_handle, tint_handle) =
        (builds.clone(), width.handle(), color.handle(), tint.handle());
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let counted = counted.clone();
        Box!(
            modifier =
                Modifier.size((width_handle.get().dp(), 20.dp())).draw_with_cache(move |cache| {
                    counted.set(counted.get() + 1);
                    let size = cache.size();
                    // Read by the cache's block: a change builds it again.
                    let color = color_handle.get();
                    cache.on_draw_behind(move |scope| {
                        // Read while drawing: a change only draws again.
                        let alpha = tint_handle.get();
                        scope.draw_rect(
                            Rect::new(0.0, 0.0, size.width, size.height),
                            Paint::fill(color).with_alpha(alpha),
                        );
                    })
                })
        );
    });
    assert_eq!(builds.get(), 1);
    tint.set(0.5);
    ui.wait_for_idle();
    assert_eq!(builds.get(), 1);
    assert!(ui.dump_display_list().contains("rect 0,0 50x20 #80FF0000"));
    color.set(Color::BLUE);
    ui.wait_for_idle();
    assert_eq!(builds.get(), 2);
    assert!(ui.dump_display_list().contains("rect 0,0 50x20 #800000FF"));
    width.set(60.0);
    ui.wait_for_idle();
    assert_eq!(builds.get(), 3);
    assert!(ui.dump_display_list().contains("rect 0,0 60x20 #800000FF"));
}
