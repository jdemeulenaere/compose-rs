//! Paths changed in place (D530–D533): a `Path` remembered and reset, as Compose code does. A
//! drawing records the path's contents as they are when it draws, so it keeps what it drew until
//! it's drawn again for its own reasons (state it read, its inputs), as a render node keeps its
//! copy of an `android.graphics.Path`: changing a path draws nothing again (D531), and a group's
//! key and version name one drawing (D157).

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

/// A path changed outside any drawing (a handler, an effect) draws nothing again, as in Compose:
/// the group keeps what it drew, with its version, until it's drawn again for its own reasons,
/// and then draws the path as it is.
#[test]
fn a_change_outside_drawing_draws_nothing_again() {
    let path = Path::new();
    path.move_to(0.0, 0.0);
    path.line_to(10.0, 0.0);
    let shared = path.clone();
    let tick = mutable_state_of(0u32);
    let handle = tick.handle();
    let mut host = host(move || {
        let shared = shared.clone();
        Column!({
            let shared = shared.clone();
            Leaf(
                Modifier
                    .repaint_boundary()
                    .draw_behind(move |scope| {
                        // Read, so a new tick draws it again.
                        let _ = handle.get() / 2;
                        scope.draw_path(&shared, Paint::stroke(Color::BLACK, 1.0))
                    })
                    .size(10.dp()),
            );
        });
    });
    assert_eq!(drawn_paths(&host), [2]);
    let before = versions(&host);
    path.line_to(10.0, 10.0);
    host.frame();
    assert!(!host.has_pending_work());
    assert_eq!(versions(&host), before, "kept, with its version");
    assert_eq!(drawn_paths(&host), [2], "what it drew");
    tick.set(1);
    host.frame();
    assert_ne!(versions(&host), before);
    assert_eq!(drawn_paths(&host), [3], "drawn again, the path as it is");
}

/// A drawing resetting a path another drawing drew: each keeps the contents it drew, nothing
/// is drawn again for it, and no frame is asked for.
#[test]
fn drawings_sharing_a_path_keep_what_they_drew() {
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
    assert_eq!(drawn_paths(&host), [0, 2]);
    assert!(!host.has_pending_work());
    let first_version = versions(&host)[0];
    segments.set(3);
    host.frame();
    assert_eq!(drawn_paths(&host), [0, 4], "each drawing keeps what it drew");
    assert_eq!(versions(&host)[0], first_version);
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

/// A path changed in composition draws again what drew it only if the drawing is drawn again
/// for its own reasons, as in Compose: here composition makes the drawing anew (its lambda
/// captures the state's value), so it is.
#[test]
fn a_path_changed_in_composition_is_drawn_by_a_drawing_made_anew() {
    let width = mutable_state_of(10.0f32);
    let handle = width.handle();
    let mut host = host(move || {
        let path = remember(Path::new);
        let width = handle.get();
        path.reset();
        path.move_to(0.0, 0.0);
        path.line_to(width, 0.0);
        if width > 10.0 {
            path.line_to(width, width);
        }
        Leaf(
            Modifier
                .repaint_boundary()
                .draw_behind(move |scope| {
                    let _ = width;
                    scope.draw_path(&path, Paint::stroke(Color::BLACK, 1.0))
                })
                .size(10.dp()),
        );
    });
    assert_eq!(drawn_paths(&host), [2]);
    width.set(20.0);
    host.frame();
    assert_eq!(drawn_paths(&host), [3]);
    assert!(!host.has_pending_work());
}

/// Two drawings resetting one path to the same shape settle at once (a review found they drew
/// each other again forever while D531 drew again what drew a changed path).
#[test]
fn two_drawings_resetting_one_path_settle() {
    let path = Path::new();
    let (first, second) = (path.clone(), path.clone());
    let reset_and_draw = |path: Path| {
        Modifier
            .repaint_boundary()
            .draw_behind(move |scope| {
                path.reset();
                path.move_to(0.0, 0.0);
                path.line_to(5.0, 5.0);
                scope.draw_path(&path, Paint::stroke(Color::BLACK, 1.0));
            })
            .size(10.dp())
    };
    let mut host = host(move || {
        let (first, second) = (first.clone(), second.clone());
        Column!({
            Leaf(reset_and_draw(first.clone()));
            Leaf(reset_and_draw(second.clone()));
        });
    });
    let mut frames = 0;
    while host.has_pending_work() && frames < 100 {
        host.frame();
        frames += 1;
    }
    assert!(!host.has_pending_work(), "still drawing after {frames} frames");
}

/// A path combined with itself in a loop (a union of many shapes, a common Compose pattern) and
/// dropped, on a small stack: its operations are a flat list of steps (D535; a review found they
/// nested, and dropping them recursed once per operation).
#[test]
fn a_long_chain_of_operations_drops() {
    let handle = std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(|| {
            let union = Path::new();
            let shape = Path::new();
            let count: u32 =
                std::env::var("CHAIN").ok().and_then(|n| n.parse().ok()).unwrap_or(100_000);
            for index in 0..count {
                shape.reset();
                shape.add_rect(compose_ui::Rect::new(index as f32, 0.0, index as f32 + 1.0, 1.0));
                union.op(&union, &shape, PathOperation::Union);
            }
            drop(union);
        })
        .unwrap();
    assert!(handle.join().is_ok());
}

// `PathData` isn't `Clone` (a review found a copy took its original's identity and generation):
// a `compile_fail` example on `PathData` checks it.

/// Timing (`--release --ignored --nocapture`): frames with nothing changed, `n` boundaries each
/// drawing its own remembered `Path`, against fixed contents: the same cost since D531 checks
/// nothing per path.
#[test]
#[ignore]
fn timing_idle_frames_with_many_path_drawings() {
    for (n, mutable) in [(1000usize, false), (1000, true)] {
        let tick = mutable_state_of(0u32);
        let handle = tick.handle();
        let mut host = host(move || {
            Column!({
                for _ in 0..n {
                    let path = remember(|| {
                        let path = Path::new();
                        path.move_to(0.0, 0.0);
                        path.line_to(1.0, 1.0);
                        path
                    });
                    let data = path.data();
                    Leaf(
                        Modifier
                            .repaint_boundary()
                            .draw_behind(move |scope| {
                                if mutable {
                                    scope.draw_path(&path, Paint::stroke(Color::BLACK, 1.0))
                                } else {
                                    scope.draw_path(&data, Paint::stroke(Color::BLACK, 1.0))
                                }
                            })
                            .size(1.dp()),
                    );
                }
                Leaf(
                    Modifier
                        .repaint_boundary()
                        .then(painted(move || {
                            if handle.get().is_multiple_of(2) { Color::RED } else { Color::BLUE }
                        }))
                        .size(1.dp()),
                );
            });
        });
        let frames = 200;
        let start = std::time::Instant::now();
        for frame in 0..frames {
            tick.set(frame + 1);
            host.frame();
        }
        println!("n = {n}, mutable {mutable}: {:?} a frame", start.elapsed() / frames);
    }
}
