//! Review of D270: the display list as a tree, recordings shared by `Arc`.

#![allow(non_snake_case)]

use std::sync::{Arc, Weak};

use crate::support::*;

use compose_runtime::*;
use compose_ui::platform::*;
use compose_ui::*;

fn painted(color: impl Fn() -> Color + 'static) -> Modifier {
    Modifier.draw_behind(move |scope| {
        let bounds = scope.bounds();
        scope.draw_rect(bounds, Paint::fill(color()));
    })
}

fn fresh_dump(content: impl Fn() + 'static) -> String {
    host(content).display_list().dump()
}

/// A clone of a frame (as a renderer keeping the last frame for damage would hold) keeps what
/// it drew when the next frame draws a nested boundary again, its parent kept.
#[test]
fn a_held_frame_keeps_its_nested_recordings() {
    let color = mutable_state_of(Color::RED);
    let handle = color.handle();
    let mut host = host(move || {
        Column!(modifier = Modifier.repaint_boundary(), {
            Leaf(Modifier.size(5.dp()).then(painted(|| Color::BLUE)));
            Leaf(Modifier.repaint_boundary().then(painted(move || handle.get())).size(10.dp()));
        });
    });
    let held: DisplayList = host.with_tree(|tree| tree.display_list().clone());
    let before = held.dump();
    assert!(before.contains("#FFFF0000"), "{before}");
    color.set(Color::GREEN);
    host.frame();
    let after = host.display_list().dump();
    assert!(after.contains("#FF00FF00"), "{after}");
    assert_eq!(held.dump(), before, "the held frame changed");
    // And again, the held frame dropped: in place.
    drop(held);
    let pointer = host.with_tree(|tree| Arc::as_ptr(&tree.display_list().groups[1].content));
    color.set(Color::BLUE);
    host.frame();
    let now = host.with_tree(|tree| Arc::as_ptr(&tree.display_list().groups[1].content));
    assert_eq!(pointer, now, "recorded in place when unshared");
    assert!(host.display_list().dump().contains("#FF0000FF"));
}

/// A removed boundary's recording is freed: no frame keeps it.
#[test]
fn a_removed_boundarys_recording_is_freed() {
    let shown = mutable_state_of(true);
    let handle = shown.handle();
    let mut host = host(move || {
        Column!(modifier = Modifier.repaint_boundary(), {
            if handle.get() {
                Leaf(Modifier.repaint_boundary().then(painted(|| Color::RED)).size(10.dp()));
            }
            Leaf(Modifier.repaint_boundary().then(painted(|| Color::BLUE)).size(10.dp()));
        });
    });
    let weak: Weak<DisplayList> = host.with_tree(|tree| {
        assert_eq!(tree.display_list().groups.len(), 3);
        Arc::downgrade(&tree.display_list().groups[1].content)
    });
    assert!(weak.upgrade().is_some());
    shown.set(false);
    host.frame();
    host.with_tree(|tree| assert_eq!(tree.display_list().groups.len(), 2));
    assert!(weak.upgrade().is_none(), "the removed boundary's recording is still alive");
}

/// A child that stops being a boundary, its parent boundary kept or not: the frame draws what
/// a fresh one does.
#[test]
fn a_child_that_stops_being_a_boundary_draws_as_a_fresh_frame() {
    let boundary = mutable_state_of(true);
    let handle = boundary.handle();
    let content = move || {
        Column!(modifier = Modifier.repaint_boundary(), {
            Leaf(Modifier.size(5.dp()).then(painted(|| Color::BLUE)));
            let modifier = if handle.get() { Modifier.repaint_boundary() } else { Modifier };
            Leaf(modifier.then(painted(|| Color::RED)).size(10.dp()));
        });
    };
    let mut host = host(content);
    for value in [false, true, false] {
        boundary.set(value);
        host.frame();
        let expected = {
            let fresh = mutable_state_of(value);
            let handle = fresh.handle();
            fresh_dump(move || {
                Column!(modifier = Modifier.repaint_boundary(), {
                    Leaf(Modifier.size(5.dp()).then(painted(|| Color::BLUE)));
                    let modifier =
                        if handle.get() { Modifier.repaint_boundary() } else { Modifier };
                    Leaf(modifier.then(painted(|| Color::RED)).size(10.dp()));
                });
            })
        };
        assert_eq!(host.display_list().dump(), expected, "boundary {value}");
    }
}

/// Content drawn twice (`draw_with_content` calling `draw_content` twice) names its boundary
/// twice: the tree's groups stay in step with the boundaries naming them.
#[test]
fn content_drawn_twice_names_its_boundary_twice() {
    let color = mutable_state_of(Color::RED);
    let handle = color.handle();
    let mut host = host(move || {
        Column!(
            modifier = Modifier.draw_with_content(|scope| {
                scope.draw_content();
                scope.draw_content();
            }),
            {
                Leaf(Modifier.repaint_boundary().then(painted(move || handle.get())).size(10.dp()));
            }
        );
    });
    let groups = |host: &UiHost| {
        host.with_tree(|tree| {
            let list = tree.display_list();
            (list.boundaries.len(), list.groups.len(), list.flatten().groups.len())
        })
    };
    assert_eq!(groups(&host), (2, 2, 2));
    color.set(Color::GREEN);
    host.frame();
    assert_eq!(groups(&host), (2, 2, 2));
    let dump = host.display_list().dump();
    assert_eq!(dump.matches("#FF00FF00").count(), 2, "{dump}");
}

#[test]
fn display_lists_are_send_and_sync() {
    fn check<T: Send + Sync>() {}
    check::<DisplayList>();
    check::<DrawGroup>();
}
