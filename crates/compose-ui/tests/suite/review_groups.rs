//! Adversarial review of commit fe43b80 (repaint boundaries as display list groups): each group
//! must replay the same in a render node of its own (its saves balanced, its translations net
//! zero, nothing restored that it didn't save) and stay the same when only its position changes.

#![allow(non_snake_case)]

use crate::support::*;

use compose_runtime::*;
use compose_ui::platform::*;
use compose_ui::*;

/// The drawings of `list`, each with the translation it's drawn at; with `isolated`, each group
/// replays as Android's render node does: from its own canvas state, what it leaves (an
/// unbalanced translation or save) dropped at its end, a restore below its start ignored.
fn drawings(list: &FlatDisplayList, isolated: bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut at = (0.0f32, 0.0f32);
    let mut stack: Vec<(f32, f32)> = Vec::new();
    // Open groups: their end, the translation and save depth at their start.
    let mut open: Vec<(usize, (f32, f32), usize)> = Vec::new();
    let mut groups = list.groups.clone();
    groups.sort_by_key(|group| (group.start, std::cmp::Reverse(group.end)));
    let mut next = 0;
    for (index, op) in list.ops.iter().enumerate() {
        while let Some(&(end, from, depth)) = open.last() {
            if end > index {
                break;
            }
            open.pop();
            if isolated {
                at = from;
                stack.truncate(depth);
            }
        }
        while next < groups.len() && groups[next].start == index {
            open.push((groups[next].end, at, stack.len()));
            next += 1;
        }
        let floor = if isolated { open.last().map_or(0, |open| open.2) } else { 0 };
        match op {
            DrawOp::Save | DrawOp::SaveLayerAlpha(_) => stack.push(at),
            DrawOp::Restore => {
                if stack.len() > floor {
                    at = stack.pop().unwrap();
                }
            }
            DrawOp::Translate(by) => at = (at.0 + by.x, at.1 + by.y),
            other => out.push(format!("{:?} at {:?}", other, at)),
        }
    }
    out
}

/// Each group: nested in the others, its saves balanced, its translations net zero.
pub(crate) fn check_groups(list: &FlatDisplayList) {
    let dump = list.dump();
    for group in &list.groups {
        assert!(group.start < group.end && group.end <= list.ops.len(), "{group:?}\n{dump}");
        // A layer's operations around the content: saves and what they apply, then as many
        // restores; none without a layer.
        let before = &list.ops[group.layer_start..group.start];
        let after = &list.ops[group.end..group.layer_end];
        let saves = before
            .iter()
            .filter(|op| {
                assert!(
                    matches!(
                        op,
                        DrawOp::Save
                            | DrawOp::Transform { .. }
                            | DrawOp::ClipRect(_)
                            | DrawOp::SaveLayerAlpha(_)
                    ),
                    "{group:?} applies its layer with {op:?}\n{dump}"
                );
                matches!(op, DrawOp::Save | DrawOp::SaveLayerAlpha(_))
            })
            .count();
        assert!(after.iter().all(|op| *op == DrawOp::Restore), "{group:?}\n{dump}");
        assert_eq!(saves, after.len(), "{group:?}\n{dump}");
        if group.layer.is_none() {
            assert!(before.is_empty() && after.is_empty(), "{group:?}\n{dump}");
        }
        let (mut depth, mut x, mut y) = (0i32, 0.0f32, 0.0f32);
        for op in &list.ops[group.start..group.end] {
            match op {
                DrawOp::Save | DrawOp::SaveLayerAlpha(_) => depth += 1,
                DrawOp::Restore => depth -= 1,
                DrawOp::Translate(by) => {
                    x += by.x;
                    y += by.y;
                }
                _ => {}
            }
            assert!(depth >= 0, "{group:?} restores what it didn't save\n{dump}");
        }
        assert_eq!(depth, 0, "{group:?} leaves a save\n{dump}");
        assert_eq!((x, y), (0.0, 0.0), "{group:?} leaves a translation\n{dump}");
        for other in &list.groups {
            let disjoint = other.end <= group.start || group.end <= other.start;
            let nested = (other.start >= group.start && other.end <= group.end)
                || (group.start >= other.start && group.end <= other.end);
            assert!(disjoint || nested, "{group:?} and {other:?} overlap\n{dump}");
        }
    }
    assert_eq!(drawings(list, false), drawings(list, true), "{dump}");
}

fn group_ops(list: &FlatDisplayList, key: u64) -> Option<Vec<DrawOp>> {
    list.groups
        .iter()
        .find(|group| group.key == key)
        .map(|group| list.ops[group.start..group.end].to_vec())
}

/// Nested and empty boundaries, layers clipping and moving, boundaries in padding and in rows:
/// every group replays the same isolated.
#[test]
fn groups_replay_the_same_in_render_nodes_of_their_own() {
    let host = host(|| {
        Column!(modifier = Modifier.repaint_boundary().padding(3.dp()), {
            Leaf(Modifier.size(4.dp()));
            // Empty boundaries: nothing drawn, in a row moved by padding.
            Row!(modifier = Modifier.padding(2.dp()), {
                Leaf(Modifier.repaint_boundary().size(5.dp()));
                Leaf(Modifier.repaint_boundary().padding(1.dp()).size(5.dp()));
                Leaf(Modifier.background(Color::RED).size(5.dp()));
            });
            // A boundary in a boundary in a clipped, moved, faded layer.
            Column!(
                modifier = Modifier
                    .padding(2.dp())
                    .graphics_layer(|layer| {
                        layer.alpha = 0.5;
                        layer.translation_x = 3.0;
                        layer.clip = true;
                    })
                    .padding(1.dp()),
                {
                    Row!(modifier = Modifier.repaint_boundary().padding(2.dp()), {
                        Leaf(Modifier.repaint_boundary().background(Color::BLUE).size(3.dp()));
                        Leaf(Modifier.size(3.dp()));
                        Leaf(Modifier.repaint_boundary().size(3.dp()));
                    });
                    Leaf(Modifier.clip(Shape::Rectangle).background(Color::GREEN).size(3.dp()));
                },
            );
            Text!("text", modifier = Modifier.repaint_boundary().padding(1.dp()));
            Leaf(Modifier.repaint_boundary());
        });
    });
    let list = host.display_list();
    assert!(list.groups.len() >= 4, "{}", list.dump());
    check_groups(&list);
}

/// A boundary moved by what's before it keeps its operations (the premise of keeping its
/// render node while it scrolls).
#[test]
fn a_moved_boundary_keeps_its_operations() {
    let height = mutable_state_of(10);
    let handle = height.handle();
    let mut host = host(move || {
        Column!({
            Leaf(Modifier.size((10.dp(), handle.get().dp())));
            Leaf(Modifier.repaint_boundary().padding(2.dp()).background(Color::RED).size(5.dp()));
        });
    });
    let key = host.display_list().groups[0].key;
    let before = group_ops(&host.display_list(), key).unwrap();
    height.set(17);
    host.frame();
    let list = host.display_list();
    check_groups(&list);
    assert_eq!(group_ops(&list, key).unwrap(), before, "{}", list.dump());
}
