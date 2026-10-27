//! Retained drawing per repaint boundary (docs/06-ui.md, "Repaint boundaries"): a boundary
//! keeps what it drew, drawing it again only when something it drew from changed (its version
//! changes then); a graphics layer's properties are its group's, outside its content, so a
//! layer-only change draws nothing again.

#![allow(non_snake_case)]

use crate::support::*;

use compose_runtime::*;
use compose_ui::platform::*;
use compose_ui::*;

/// The group of the node with key `key`.
fn group(list: &FlatDisplayList, key: u64) -> FlatGroup {
    list.groups
        .iter()
        .find(|group| group.key == key)
        .unwrap_or_else(|| panic!("no group {key}\n{}", list.dump()))
        .clone()
}

/// The groups' keys, in the order they start.
fn keys(list: &FlatDisplayList) -> Vec<u64> {
    let mut groups = list.groups.clone();
    groups.sort_by_key(|group| (group.start, std::cmp::Reverse(group.end)));
    groups.iter().map(|group| group.key).collect()
}

fn version(list: &FlatDisplayList, key: u64) -> u64 {
    group(list, key).version
}

fn dump_of(ops: &[DrawOp]) -> String {
    FlatDisplayList { ops: ops.to_vec(), ..FlatDisplayList::default() }.dump()
}

/// A leaf filling itself with what `color` gives, logging `name` when it draws.
fn painted(name: &'static str, color: impl Fn() -> Color + 'static) -> Modifier {
    Modifier.draw_behind(move |scope| {
        log(name);
        let bounds = scope.bounds();
        scope.draw_rect(bounds, Paint::fill(color()));
    })
}

/// A state read while drawing one boundary draws that boundary again, not its sibling: its
/// version changes, the sibling's stays, and the sibling's draw code doesn't run.
#[test]
fn a_state_read_in_a_boundary_draws_only_that_boundary_again() {
    let color = mutable_state_of(Color::RED);
    let handle = color.handle();
    let mut host = host(move || {
        Column!({
            Leaf(
                Modifier
                    .repaint_boundary()
                    .then(painted("first", move || handle.get()))
                    .size(10.dp()),
            );
            Leaf(Modifier.repaint_boundary().then(painted("second", || Color::BLUE)).size(10.dp()));
        });
    });
    let before = host.display_list();
    let [first, second] = keys(&before)[..] else { panic!("{}", before.dump()) };
    take_log();
    color.set(Color::GREEN);
    host.frame();
    let after = host.display_list();
    assert_eq!(take_log(), ["first"]);
    assert_ne!(version(&after, first), version(&before, first));
    assert_eq!(version(&after, second), version(&before, second));
    assert!(after.dump().contains("#FF00FF00"), "{}", after.dump());
    assert_eq!(after.dump(), fresh(|| Color::GREEN).dump());

    fn fresh(color: fn() -> Color) -> FlatDisplayList {
        crate::support::host(move || {
            Column!({
                Leaf(Modifier.repaint_boundary().then(painted("first", color)).size(10.dp()));
                Leaf(
                    Modifier
                        .repaint_boundary()
                        .then(painted("second", || Color::BLUE))
                        .size(10.dp()),
                );
            });
        })
        .display_list()
    }
}

/// A layer's alpha and rotation read in its block are its group's properties: changing them
/// draws nothing again and changes no version (neither the layer's nor its parent's), only the
/// properties and the operations applying them, outside the layer's content.
#[test]
fn an_animated_layer_changes_no_version_only_its_properties() {
    let alpha = mutable_state_of(1.0f32);
    let rotation = mutable_state_of(0.0f32);
    let (alpha_handle, rotation_handle) = (alpha.handle(), rotation.handle());
    let mut host = host(move || {
        Column!(modifier = Modifier.repaint_boundary(), {
            Leaf(Modifier.size(10.dp()));
            Leaf(
                Modifier
                    .padding(2.dp())
                    .graphics_layer(move |layer| {
                        layer.alpha = alpha_handle.get();
                        layer.rotation_z = rotation_handle.get();
                    })
                    .then(painted("layer", || Color::RED))
                    .size(10.dp()),
            );
        });
    });
    let before = host.display_list();
    let [outer, layer] = keys(&before)[..] else { panic!("{}", before.dump()) };
    let properties = group(&before, layer).layer.expect("a layer's properties");
    assert_eq!((properties.alpha, properties.rotation), (1.0, 0.0));
    // Nothing to apply: no operations around the content.
    let lone = group(&before, layer);
    assert_eq!((lone.layer_start, lone.layer_end), (lone.start, lone.end));
    take_log();
    alpha.set(0.5);
    rotation.set(30.0);
    host.frame();
    let after = host.display_list();
    assert_eq!(take_log(), Vec::<String>::new(), "the layer's content isn't drawn again");
    assert_eq!(version(&after, layer), version(&before, layer));
    assert_eq!(version(&after, outer), version(&before, outer));
    let changed = group(&after, layer);
    let properties = changed.layer.unwrap();
    assert_eq!((properties.alpha, properties.rotation), (0.5, 30.0));
    // Pivoted at the center of the layer's 10 × 10 inside the padding.
    assert_eq!(properties.pivot, Offset::new(7.0, 7.0));
    assert_eq!(
        dump_of(&after.ops[changed.layer_start..changed.start]),
        "save\n  transform pivot 7,7 scale 1x1 rotate 30 translate 0,0\n  save layer alpha 0.5\n"
    );
    assert_eq!(dump_of(&after.ops[changed.end..changed.layer_end]), "restore\nrestore\n");
    // The content is the same operations.
    assert_eq!(
        after.ops[changed.start..changed.end],
        before.ops[group(&before, layer).start..group(&before, layer).end]
    );
    assert_eq!(after.dump(), fresh(0.5, 30.0).dump());

    fn fresh(alpha: f32, rotation: f32) -> FlatDisplayList {
        crate::support::host(move || {
            Column!(modifier = Modifier.repaint_boundary(), {
                Leaf(Modifier.size(10.dp()));
                Leaf(
                    Modifier
                        .padding(2.dp())
                        .graphics_layer(move |layer| {
                            layer.alpha = alpha;
                            layer.rotation_z = rotation;
                        })
                        .then(painted("layer", || Color::RED))
                        .size(10.dp()),
                );
            });
        })
        .display_list()
    }
}

/// A layer whose node draws before it (a background under it) is a group of its own inside what
/// draws its node (an inner layer, D522), its properties computed while drawing the node: its
/// block's reads draw the node again.
#[test]
fn a_layer_after_a_drawing_is_part_of_its_nodes_content() {
    let alpha = mutable_state_of(1.0f32);
    let handle = alpha.handle();
    let mut host = host(move || {
        Leaf(
            Modifier
                .background(Color::BLUE)
                .graphics_layer(move |layer| layer.alpha = handle.get())
                .then(painted("layer", || Color::RED))
                .size(10.dp()),
        );
    });
    let before = host.display_list();
    // The node isn't a boundary: the frame draws the background, then the layer's group.
    assert_eq!(before.groups.len(), 1, "{}", before.dump());
    let layer = before.groups[0].clone();
    assert_eq!(layer.layer.map(|layer| layer.alpha), Some(1.0));
    take_log();
    alpha.set(0.5);
    host.frame();
    let after = host.display_list();
    assert_eq!(take_log(), ["layer"]);
    assert_ne!(version(&after, layer.key), layer.version);
    assert!(after.dump().contains("save layer alpha 0.5"), "{}", after.dump());
}

/// A nested boundary drawn again changes its parent's version (the parent's operations hold
/// its), without drawing the parent's own content again: its kept operations are spliced.
#[test]
fn a_nested_boundary_drawn_again_changes_its_parents_version() {
    let color = mutable_state_of(Color::RED);
    let handle = color.handle();
    let mut host = host(move || {
        Column!(modifier = Modifier.repaint_boundary(), {
            Leaf(
                Modifier
                    .repaint_boundary()
                    .then(painted("inner", move || handle.get()))
                    .size(10.dp()),
            );
            Leaf(Modifier.then(painted("outer", || Color::BLUE)).size(10.dp()));
        });
    });
    let before = host.display_list();
    let [outer, inner] = keys(&before)[..] else { panic!("{}", before.dump()) };
    take_log();
    color.set(Color::GREEN);
    host.frame();
    let after = host.display_list();
    assert_eq!(take_log(), ["inner"]);
    assert_ne!(version(&after, inner), version(&before, inner));
    assert_ne!(version(&after, outer), version(&before, outer));
}

/// A boundary moved by what's before it keeps its version (its position is its parent's
/// operations), and isn't drawn again; the parent's version changes.
#[test]
fn moving_a_boundary_changes_not_its_version() {
    let height = mutable_state_of(10);
    let handle = height.handle();
    // Made once, so the recomposition moving it gives it the same modifier.
    let boundary = Modifier.repaint_boundary().then(painted("moved", || Color::RED)).size(5.dp());
    let mut host = host(move || {
        let boundary = boundary.clone();
        Column!(modifier = Modifier.repaint_boundary(), {
            Leaf(Modifier.size((10.dp(), handle.get().dp())));
            Leaf(boundary.clone());
        });
    });
    let before = host.display_list();
    let [outer, moved] = keys(&before)[..] else { panic!("{}", before.dump()) };
    take_log();
    height.set(17);
    host.frame();
    let after = host.display_list();
    assert_eq!(take_log(), Vec::<String>::new());
    assert_eq!(version(&after, moved), version(&before, moved));
    assert_ne!(version(&after, outer), version(&before, outer));
    assert!(after.dump().contains("translate 0 17"), "{}", after.dump());
}

/// A node remeasured inside a boundary, or a child added to one, draws it again.
#[test]
fn a_remeasure_or_a_new_child_inside_a_boundary_draws_it_again() {
    let height = mutable_state_of(10);
    let shown = mutable_state_of(false);
    let (height_handle, shown_handle) = (height.handle(), shown.handle());
    let mut host = host(move || {
        Column!({
            Column!(modifier = Modifier.repaint_boundary(), {
                Leaf(Modifier.background(Color::RED).size((10.dp(), height_handle.get().dp())));
                if shown_handle.get() {
                    Leaf(Modifier.background(Color::GREEN).size(4.dp()));
                }
            });
            Leaf(Modifier.repaint_boundary().background(Color::BLUE).size(5.dp()));
        });
    });
    let before = host.display_list();
    let [changing, sibling] = keys(&before)[..] else { panic!("{}", before.dump()) };
    height.set(12);
    host.frame();
    let resized = host.display_list();
    assert_ne!(version(&resized, changing), version(&before, changing));
    assert_eq!(version(&resized, sibling), version(&before, sibling));
    assert!(resized.dump().contains("rect 0,0 10x12"), "{}", resized.dump());
    shown.set(true);
    host.frame();
    let added = host.display_list();
    assert_ne!(version(&added, changing), version(&resized, changing));
    assert!(added.dump().contains("#FF00FF00"), "{}", added.dump());
}

/// A press (and a hover) drawn by a clickable's indication inside a boundary draws that boundary
/// again.
#[test]
fn an_indication_inside_a_boundary_draws_it_again() {
    let mut host = host(|| {
        Column!({
            Leaf(
                Modifier
                    .repaint_boundary()
                    .clickable_with(
                        ClickableOptions {
                            indication: Indication::bounded(Color::RED),
                            ..ClickableOptions::DEFAULT
                        },
                        || {},
                    )
                    .background(Color::GREEN)
                    .size(10.dp()),
            );
            Leaf(Modifier.repaint_boundary().background(Color::BLUE).size(10.dp()));
        });
    });
    let before = host.display_list();
    let [pressed, sibling] = keys(&before)[..] else { panic!("{}", before.dump()) };
    host.send_pointer(PointerEvent::press(Offset::new(5.0, 5.0)));
    host.frame();
    let during = host.display_list();
    assert_ne!(version(&during, pressed), version(&before, pressed));
    assert_eq!(version(&during, sibling), version(&before, sibling));
    assert_ne!(during.dump(), before.dump());
    host.send_pointer(PointerEvent::release(Offset::new(5.0, 5.0)));
    host.frame();
    // Hovered still.
    assert_ne!(host.display_list().dump(), during.dump());
    host.send_pointer_left();
    host.frame();
    assert_eq!(host.display_list().dump(), before.dump());
}

/// An explicit `invalidate_draw`, or a new density, draws every boundary again.
#[test]
fn invalidating_the_drawing_or_the_density_draws_every_boundary_again() {
    let mut host = host(|| {
        Column!(modifier = Modifier.repaint_boundary(), {
            Leaf(Modifier.repaint_boundary().then(painted("inner", || Color::RED)).size(10.dp()));
        });
    });
    let before = host.display_list();
    take_log();
    host.with_tree(|tree| tree.invalidate_draw());
    host.frame();
    let after = host.display_list();
    assert_eq!(take_log(), ["inner"]);
    for key in keys(&before) {
        assert_ne!(version(&after, key), version(&before, key));
    }
    host.set_density(Density::new(2.0));
    host.frame();
    let denser = host.display_list();
    assert_eq!(take_log(), ["inner"]);
    for key in keys(&before) {
        assert_ne!(version(&denser, key), version(&after, key));
    }
}

// ---- Incremental frames draw what a fresh draw does ------------------------------------------

/// What a scene draws from: four numbers, each a state.
#[derive(Clone, Copy)]
struct Knobs([MutableState<i32>; 4]);

fn tint(value: i32) -> Color {
    Color::from_argb(0xFF00_0000 | (value as u32).wrapping_mul(0x0012_3457) & 0x00FF_FFFF)
}

/// Boundaries drawing state, nested, moved, in layers lifted out of their content and not, an
/// empty layer, a child coming and going, and drawing outside any boundary.
fn scene(knobs: Knobs) {
    let [a, b, c, d] = knobs.0;
    Column!(modifier = Modifier.repaint_boundary().padding(2.dp()), {
        Leaf(Modifier.repaint_boundary().then(painted("a", move || tint(a.get()))).size(6.dp()));
        Leaf(Modifier.background(Color::BLUE).size((5.dp(), b.get().dp())));
        Row!(
            modifier = Modifier.padding(1.dp()).graphics_layer(move |layer| {
                layer.alpha = c.get() as f32 / 10.0;
                layer.rotation_z = c.get() as f32 * 3.0;
                layer.clip = c.get() % 2 == 0;
            }),
            {
                Leaf(
                    Modifier
                        .repaint_boundary()
                        .then(painted("d", move || tint(d.get())))
                        .size((4.dp(), (d.get() % 4 + 2).dp())),
                );
                if d.get() % 3 != 0 {
                    Leaf(Modifier.background(Color::GREEN).size(3.dp()));
                }
                Leaf(
                    Modifier
                        .graphics_layer(move |layer| layer.translation_y = a.get() as f32)
                        .alpha(0.5)
                        .background(Color::RED)
                        .size(4.dp()),
                );
            },
        );
        Leaf(
            Modifier
                .background(Color::RED)
                .graphics_layer(move |layer| layer.scale_x = 1.0 + c.get() as f32 / 10.0)
                .then(painted("scaled", move || tint(b.get())))
                .size(5.dp()),
        );
        Leaf(
            Modifier.graphics_layer(move |layer| layer.alpha = c.get() as f32 / 20.0).size(3.dp()),
        );
        // A node becoming a boundary, and no longer one.
        let toggled = if a.get() % 2 == 0 { Modifier.repaint_boundary() } else { Modifier };
        Leaf(toggled.background(tint(a.get())).size(2.dp()));
        // A boundary measured again with its parent's new constraints, its modifier the same.
        Column!(modifier = Modifier.width((b.get() + 2).dp()), {
            Leaf(
                Modifier
                    .repaint_boundary()
                    .fill_max_width()
                    .background(Color::GREEN)
                    .height(2.dp()),
            );
        });
        // A boundary larger than its parent lets it, centered there: moved by its own size, its
        // parent not measured again.
        Stack!(modifier = Modifier.size(4.dp()), {
            Leaf(
                Modifier
                    .repaint_boundary()
                    .background(Color::BLUE)
                    .required_size(((d.get() * 2 + 4).dp(), 6.dp())),
            );
        });
        Leaf(
            Modifier
                .repaint_boundary()
                .draw_with_content(move |scope| {
                    if b.get() % 2 == 0 {
                        scope.draw_content();
                    }
                })
                .background(Color::GREEN)
                .size(4.dp()),
        );
    });
    Leaf(Modifier.then(painted("outside", move || tint(a.get() + d.get()))).size(3.dp()));
}

/// The operations of `group`, without those applying the layers of the groups inside it (which
/// a renderer takes from their `layer`): what its version stands for.
fn versioned_ops(list: &FlatDisplayList, group: &FlatGroup) -> Vec<DrawOp> {
    let skipped = |index: usize| {
        list.groups.iter().any(|inner| {
            inner.key != group.key
                && inner.layer_start >= group.start
                && inner.layer_end <= group.end
                && ((inner.layer_start..inner.start).contains(&index)
                    || (inner.end..inner.layer_end).contains(&index))
        })
    };
    (group.start..group.end)
        .filter(|index| !skipped(*index))
        .map(|index| list.ops[index].clone())
        .collect()
}

/// The groups' shapes, without their keys and versions (a fresh host's nodes are others).
fn shapes(list: &FlatDisplayList) -> Vec<(usize, usize, usize, usize, Option<LayerProperties>)> {
    let mut shapes: Vec<_> = list
        .groups
        .iter()
        .map(|group| {
            (group.layer_start, group.start, group.end, group.layer_end, group.layer.clone())
        })
        .collect();
    shapes.sort_by_key(|shape| (shape.0, shape.1, std::cmp::Reverse(shape.2)));
    shapes
}

fn fresh_scene(values: [i32; 4]) -> FlatDisplayList {
    let owned: Vec<OwnedState<i32>> = values.iter().map(|value| mutable_state_of(*value)).collect();
    let knobs = Knobs(std::array::from_fn(|index| owned[index].handle()));
    let host = host(move || scene(knobs));
    let list = host.display_list();
    drop(host);
    drop(owned);
    list
}

/// After each change, the incremental frame's display list is a fresh draw's, group for group;
/// a group whose version stayed has the same operations (but nested layers' properties).
#[test]
fn incremental_frames_draw_what_a_fresh_draw_does() {
    let mut values = [3, 10, 5, 1];
    let owned: Vec<OwnedState<i32>> = values.iter().map(|value| mutable_state_of(*value)).collect();
    let knobs = Knobs(std::array::from_fn(|index| owned[index].handle()));
    let mut host = host(move || scene(knobs));
    let mut previous = host.display_list();
    assert_eq!(previous.dump(), fresh_scene(values).dump());
    let steps = [
        (0, 8),
        (2, 6),
        (2, 10),
        (1, 13),
        (3, 2),
        (3, 3),
        (2, 4),
        (0, 1),
        (1, 12),
        (3, 4),
        (2, 0),
        (2, 7),
    ];
    for (step, &(knob, value)) in steps.iter().enumerate() {
        values[knob] = value;
        owned[knob].set(value);
        host.frame();
        let list = host.display_list();
        crate::review_groups::check_groups(&list);
        let fresh = fresh_scene(values);
        assert_eq!(list.dump(), fresh.dump(), "step {step}: {values:?}");
        assert_eq!(shapes(&list), shapes(&fresh), "step {step}: {values:?}\n{}", list.dump());
        for group in &list.groups {
            assert!(group.layer_start <= group.start && group.end <= group.layer_end);
            if group.layer.is_none() {
                assert_eq!((group.layer_start, group.layer_end), (group.start, group.end));
            }
            if let Some(before) = previous.groups.iter().find(|before| before.key == group.key)
                && before.version == group.version
            {
                assert_eq!(
                    versioned_ops(&list, group),
                    versioned_ops(&previous, before),
                    "step {step}: {values:?}, group {group:?} kept its version"
                );
            }
        }
        previous = list;
    }
}

/// A frame shares each kept boundary's recording, not a copy of its operations (D270): the
/// next frame's group holds the same recording while the boundary isn't drawn again, a new one
/// once it is. A frame kept past the next keeps what it drew.
#[test]
fn a_frame_shares_a_kept_boundarys_recording() {
    let color = mutable_state_of(Color::RED);
    let handle = color.handle();
    let mut host = host(move || {
        Column!({
            Leaf(
                Modifier
                    .repaint_boundary()
                    .then(painted("first", move || handle.get()))
                    .size(10.dp()),
            );
            Leaf(Modifier.repaint_boundary().then(painted("second", || Color::BLUE)).size(10.dp()));
        });
    });
    let content = |host: &compose_ui::platform::UiHost, index: usize| {
        host.with_tree(|tree| std::sync::Arc::clone(&tree.display_list().groups[index].content))
    };
    let (first, second) = (content(&host, 0), content(&host, 1));
    color.set(Color::GREEN);
    host.frame();
    assert!(std::sync::Arc::ptr_eq(&content(&host, 1), &second), "kept: shared");
    assert!(!std::sync::Arc::ptr_eq(&content(&host, 0), &first), "drawn again");
    // The recording an earlier frame still held is as it drew it.
    assert!(matches!(first.ops[..], [DrawOp::DrawRect(_, paint)] if paint.color == Color::RED));
    assert!(host.display_list().dump().contains("#FF00FF00"));
}
