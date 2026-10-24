//! Adversarial review of D520-D529 (graphics layers as Compose's: inner layers, clips, shadows).

#![allow(non_snake_case)]

use crate::support::*;

use compose_runtime::*;
use compose_ui::platform::*;
use compose_ui::*;

fn tint(value: i32) -> Color {
    Color::from_argb(0xFF00_0000 | (value as u32).wrapping_mul(0x0012_3457) & 0x00FF_FFFF)
}

fn painted(color: impl Fn() -> Color + 'static) -> Modifier {
    Modifier.draw_behind(move |scope| {
        let bounds = scope.bounds();
        scope.draw_rect(bounds, Paint::fill(color()));
    })
}

#[derive(Clone, Copy)]
struct Knobs([MutableState<i32>; 5]);

/// Inner layers in every position: after a drawing, two in one node with drawings between,
/// nested in another node's inner layer, around a boundary, toggled shadows, a chain changing
/// before an inner layer, a conditional draw_content around one, children coming and going.
fn scene(knobs: Knobs) {
    let [a, b, c, d, e] = knobs.0;
    Column!(modifier = Modifier.repaint_boundary(), {
        Column!(
            modifier = Modifier
                .background(Color::BLUE)
                .clip(Shape::Rounded([CornerSize::Dp(2.dp()); 4]))
                .padding(1.dp()),
            {
                Leaf(painted(move || tint(a.get())).size((4.dp(), (a.get() % 3 + 2).dp())));
                if b.get() % 2 == 0 {
                    Leaf(Modifier.background(Color::GREEN).size(2.dp()));
                }
                Row!(
                    modifier = Modifier
                        .background(Color::RED)
                        .graphics_layer(move |layer| layer.alpha = c.get() as f32 / 10.0)
                        .then(painted(move || tint(d.get()))),
                    {
                        Leaf(Modifier.repaint_boundary().then(painted(move || tint(e.get()))).size(3.dp()));
                        Leaf(painted(move || tint(a.get() + 1)).size(2.dp()));
                    }
                );
            }
        );
        Leaf(
            Modifier
                .background(Color::RED)
                .clip(Shape::Rounded([CornerSize::Dp(1.dp()); 4]))
                .then(painted(move || tint(b.get())))
                .graphics_layer(move |layer| {
                    layer.rotation_z = c.get() as f32;
                    layer.shadow_elevation = (d.get() % 2) as f32 * 3.0;
                })
                .then(painted(move || tint(d.get())))
                .size(6.dp()),
        );
        Leaf(Modifier.background(Color::WHITE).shadow((e.get() % 3).dp()).size(5.dp()));
        Leaf(Modifier.shadow((d.get() % 2 * 4).dp()).size(5.dp()));
        let pre = if a.get() % 2 == 0 { Modifier.border(1.dp(), Color::BLACK) } else { Modifier };
        Leaf(pre.background(Color::BLUE).clip_to_bounds().then(painted(move || tint(c.get()))).size(4.dp()));
        let boundary = if c.get() % 2 == 0 { Modifier.repaint_boundary() } else { Modifier };
        Column!(
            modifier = boundary
                .background(Color::GREEN)
                .clip_to_bounds()
                .draw_with_content(move |scope| {
                    scope.draw_content();
                    if e.get() % 2 == 0 {
                        let bounds = scope.bounds();
                        scope.draw_rect(bounds, Paint::fill(Color::RED));
                    }
                }),
            {
                Leaf(painted(move || tint(d.get() * 3)).size((3.dp(), (b.get() % 4 + 1).dp())));
                Leaf(
                    Modifier
                        .graphics_layer(move |layer| layer.alpha = e.get() as f32 / 10.0)
                        .background(Color::BLUE)
                        .clip(Shape::Rounded([CornerSize::Dp(1.dp()); 4]))
                        .then(painted(move || tint(a.get() * 5)))
                        .size(3.dp()),
                );
            }
        );
        Leaf(
            Modifier
                .draw_with_content(move |scope| {
                    if b.get() % 3 != 0 {
                        scope.draw_content();
                    }
                })
                .background(Color::GREEN)
                .clip_to_bounds()
                .then(painted(move || tint(e.get())))
                .size(3.dp()),
        );
    });
}

fn fresh_scene(values: [i32; 5]) -> FlatDisplayList {
    let owned: Vec<OwnedState<i32>> = values.iter().map(|value| mutable_state_of(*value)).collect();
    let knobs = Knobs(std::array::from_fn(|index| owned[index].handle()));
    let host = host(move || scene(knobs));
    let list = host.display_list();
    drop(host);
    drop(owned);
    list
}

#[test]
fn review_incremental_frames_with_inner_layers_draw_what_a_fresh_draw_does() {
    let mut values = [3, 10, 5, 1, 2];
    let owned: Vec<OwnedState<i32>> = values.iter().map(|value| mutable_state_of(*value)).collect();
    let knobs = Knobs(std::array::from_fn(|index| owned[index].handle()));
    let mut host = host(move || scene(knobs));
    assert_eq!(host.display_list().dump(), fresh_scene(values).dump());
    let mut previous = host.display_list();
    let steps = [
        (0, 8), (2, 6), (4, 7), (3, 2), (1, 13), (3, 3), (2, 4), (0, 1), (1, 12), (3, 4),
        (4, 3), (2, 0), (2, 7), (1, 9), (0, 6), (4, 5), (3, 1), (1, 3),
    ];
    for (step, &(knob, value)) in steps.iter().enumerate() {
        values[knob] = value;
        owned[knob].set(value);
        host.frame();
        let list = host.display_list();

        let fresh = fresh_scene(values);
        assert_eq!(list.dump(), fresh.dump(), "step {step}: {values:?}");
        for group in &list.groups {
            if let Some(before) = previous.groups.iter().find(|before| before.key == group.key)
                && before.version == group.version
            {
                // A kept version: the same content, but nested groups' layers.
                let ops = |list: &FlatDisplayList, group: &FlatGroup| {
                    (group.start..group.end)
                        .filter(|index| {
                            !list.groups.iter().any(|inner| {
                                inner.key != group.key
                                    && inner.layer_start >= group.start
                                    && inner.layer_end <= group.end
                                    && ((inner.layer_start..inner.start).contains(index)
                                        || (inner.end..inner.layer_end).contains(index))
                            })
                        })
                        .map(|index| list.ops[index].clone())
                        .collect::<Vec<_>>()
                };
                assert_eq!(
                    ops(&list, group),
                    ops(&previous, before),
                    "step {step}: {values:?}, group {:x} kept its version",
                    group.key
                );
            }
        }
        previous = list;
    }
}

/// Compose's `Modifier.alpha(a)` is `if (a != 1f) graphicsLayer(alpha = a, clip = true) else
/// this` (Alpha.kt), `rotate(0f)` and `scale(1f)` add no layer: the layer clips (drawing and,
/// since D523, hit testing), and identity values make no render node.
#[test]
fn review_alpha_clips_and_identity_transforms_add_no_layer_as_composes() {
    let host = host(|| {
        Column!({
            Leaf(Modifier.alpha(0.5).then(painted(|| Color::RED)).size(10.dp()));
            Leaf(Modifier.alpha(1.0).then(painted(|| Color::RED)).size(10.dp()));
            Leaf(Modifier.rotate(0.0).scale(1.0).then(painted(|| Color::RED)).size(10.dp()));
        });
    });
    let list = host.display_list();
    let layers: Vec<LayerProperties> = list.groups.iter().filter_map(|group| group.layer).collect();
    assert_eq!(layers.len(), 1, "only alpha(0.5) is a layer: {layers:?}");
    assert!(layers[0].clip, "alpha's layer clips to its bounds");
}

/// A state read in a node's drawing before its inner layer (a top app bar's container color,
/// read while drawing, then its surface's clip) changes what's before the layer only: the
/// layer's content (the bar's title) needn't draw again, as Compose's layer keeps its recording.
#[test]
fn review_a_draw_read_before_an_inner_layer_keeps_the_layers_content() {
    let color = mutable_state_of(Color::RED);
    let handle = color.handle();
    let mut host = host(move || {
        Column!(
            modifier = Modifier
                .repaint_boundary()
                .draw_behind(move |scope| {
                    let bounds = scope.bounds();
                    scope.draw_rect(bounds, Paint::fill(handle.get()));
                })
                .clip_to_bounds(),
            {
                Leaf(
                    Modifier
                        .draw_behind(|scope| {
                            log("title");
                            let bounds = scope.bounds();
                            scope.draw_rect(bounds, Paint::fill(Color::BLUE));
                        })
                        .size(10.dp()),
                );
            }
        );
    });
    take_log();
    color.set(Color::GREEN);
    host.frame();
    assert!(host.display_list().dump().contains("#FF00FF00"));
    assert_eq!(take_log(), Vec::<String>::new(), "the inner layer's content drew again");
}

/// A container with a background whose clip is an inner layer (a `LazyColumn` given
/// `Modifier.background(…)`) moving its child (a scroll: the container measures again, the
/// same size, and places the child elsewhere): its inner layer draws again, not what draws the
/// container (the boundary around it, its background), as Compose's layer is recorded again
/// alone.
#[test]
fn a_child_moving_in_an_inner_layer_keeps_what_draws_its_container() {
    let offset = mutable_state_of(0i32);
    let handle = offset.handle();
    let mut host = host(move || {
        Layout(
            Modifier.repaint_boundary().draw_behind(|scope| {
                log("outer");
                let bounds = scope.bounds();
                scope.draw_rect(bounds, Paint::fill(Color::WHITE));
            }),
            std::rc::Rc::new(StackMeasurePolicy),
            callback(move || {
                Layout(
                    Modifier.background(Color::GREEN).clip_to_bounds().size(40.dp()),
                    measure_policy(move |scope, measurables, constraints| {
                        let y = handle.get();
                        for &measurable in measurables {
                            let placeable = scope.measure(measurable, constraints.loosen());
                            scope.place(&placeable, 0, y);
                        }
                        scope.layout(constraints.max_width, constraints.max_height)
                    }),
                    callback(|| Leaf(painted(|| Color::RED).size(10.dp()))),
                );
            }),
        );
    });
    take_log();
    offset.set(7);
    host.frame();
    assert!(host.display_list().dump().contains("translate 0 7"), "{}", host.display_list().dump());
    assert_eq!(take_log(), Vec::<String>::new(), "what draws the container drew again");
}
