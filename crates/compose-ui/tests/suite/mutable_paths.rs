//! Paths changed in place (D530–D533): a `Path` remembered and reset, as Compose code does. A
//! drawing records the path's contents as they are when it draws, so it keeps what it drew; a
//! group whose path changed after it was recorded is drawn again, with a new version, so a key
//! and a version name one drawing (D157).

#![allow(non_snake_case)]

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

/// A leaf, a repaint boundary, drawing `path`.
fn drawing(path: &Path) -> Modifier {
    let path = path.clone();
    Modifier
        .repaint_boundary()
        .draw_behind(move |scope| scope.draw_path(&path, Paint::stroke(Color::BLACK, 1.0)))
        .size(10.dp())
}

/// Each group's (key, version).
fn versions(host: &UiHost) -> Vec<(u64, u64)> {
    host.with_tree(|tree| {
        tree.display_list().groups.iter().map(|group| (group.key, group.version)).collect()
    })
}

/// The commands of each path the frame draws, in order.
fn drawn_paths(host: &UiHost) -> Vec<usize> {
    host.with_tree(|tree| {
        tree.display_list()
            .flatten()
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::DrawPath { path, .. } => Some(path.commands().len()),
                _ => None,
            })
            .collect()
    })
}

/// A path changed outside any drawing (a handler, an effect) draws again the group that drew
/// it in the next frame, with a new version; unchanged, the group keeps its version.
#[test]
fn a_change_outside_drawing_draws_the_group_again() {
    let path = Path::new();
    path.move_to(0.0, 0.0);
    path.line_to(10.0, 0.0);
    let shared = path.clone();
    let tick = mutable_state_of(0u32);
    let handle = tick.handle();
    let mut host = host(move || {
        let shared = shared.clone();
        Column!({
            Leaf(drawing(&shared));
            Leaf(
                Modifier
                    .repaint_boundary()
                    .then(painted(move || {
                        if handle.get().is_multiple_of(2) { Color::RED } else { Color::BLUE }
                    }))
                    .size(10.dp()),
            );
        });
    });
    assert_eq!(drawn_paths(&host), [2]);
    let before = versions(&host);
    tick.set(1);
    host.frame();
    assert_eq!(versions(&host)[0], before[0], "unchanged, kept");
    path.line_to(10.0, 10.0);
    tick.set(2);
    host.frame();
    let after = versions(&host);
    assert_ne!(after[0], before[0], "changed, drawn again with a new version");
    assert_eq!(drawn_paths(&host), [3]);
    assert!(!host.has_pending_work());
}

/// A drawing resetting a path another drawing drew, after that one was recorded: the frame asks
/// for another, which draws the first one again as the path now is, then settles.
#[test]
fn a_path_reset_by_another_drawing_draws_the_first_again_next_frame() {
    let path = Path::new();
    let segments = mutable_state_of(1u32);
    let handle = segments.handle();
    let (first, second) = (path.clone(), path.clone());
    let mut host = host(move || {
        let (first, second) = (first.clone(), second.clone());
        Column!({
            Leaf(drawing(&first));
            let second = second.clone();
            Leaf(
                Modifier
                    .repaint_boundary()
                    .draw_behind(move |scope| {
                        second.reset();
                        second.move_to(0.0, 0.0);
                        for segment in 0..handle.get() {
                            second.line_to(segment as f32, 1.0);
                        }
                        scope.draw_path(&second, Paint::stroke(Color::BLACK, 1.0));
                    })
                    .size(10.dp()),
            );
        });
    });
    // The first leaf drew the path empty, before the second filled it.
    assert!(host.has_pending_work(), "a group drew a path older than it is");
    let stale = versions(&host);
    host.frame();
    assert_eq!(drawn_paths(&host), [2, 2]);
    assert_ne!(versions(&host)[0], stale[0]);
    assert_eq!(versions(&host)[1], stale[1], "the second wasn't drawn again");
    assert!(!host.has_pending_work(), "settled");
    segments.set(3);
    host.frame();
    assert_eq!(drawn_paths(&host), [2, 4], "each drawing keeps what it drew");
    assert!(host.has_pending_work());
    host.frame();
    assert_eq!(drawn_paths(&host), [4, 4]);
    assert!(!host.has_pending_work());
}

/// A drawing that resets its own path for each shape it draws records each shape, and isn't
/// drawn again for changing it: what it notes is the path once it's done.
#[test]
fn a_drawing_changing_its_own_path_records_each_shape_and_settles() {
    let host = host(move || {
        let path = remember(Path::new);
        Leaf(
            Modifier
                .repaint_boundary()
                .draw_behind(move |scope| {
                    for lines in 1..=3 {
                        path.reset();
                        path.move_to(0.0, 0.0);
                        for line in 0..lines {
                            path.line_to(line as f32, 5.0);
                        }
                        scope.draw_path(&path, Paint::stroke(Color::BLACK, 1.0));
                    }
                    // Changed after drawing: the next drawing starts from the reset.
                    path.reset();
                })
                .size(10.dp()),
        );
    });
    assert_eq!(drawn_paths(&host), [2, 3, 4]);
    assert!(!host.has_pending_work());
}

/// A path changed in composition (not state: nothing reads it) draws again what drew it in the
/// same frame, as a drawing reading state would be.
#[test]
fn a_path_changed_in_composition_draws_again_in_the_same_frame() {
    let width = mutable_state_of(10.0f32);
    let handle = width.handle();
    let mut host = host(move || {
        let path = remember(Path::new);
        path.reset();
        path.move_to(0.0, 0.0);
        path.line_to(handle.get(), 0.0);
        if handle.get() > 10.0 {
            path.line_to(handle.get(), handle.get());
        }
        Leaf(drawing(&path));
    });
    assert_eq!(drawn_paths(&host), [2]);
    width.set(20.0);
    host.frame();
    assert_eq!(drawn_paths(&host), [3]);
    assert!(!host.has_pending_work());
}
