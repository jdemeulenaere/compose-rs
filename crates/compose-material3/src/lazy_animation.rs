//! A value following its target on a spec, as an `animate_*_as_state`, with nothing made until the
//! target first changes: what most items of a long list never pay for.

use std::cell::Cell;
use std::rc::Rc;

use compose_animation::{Animatable, AnimationSpec};
use compose_runtime::{LaunchedEffect, key, remember};

/// From where a run started, to where it's going.
struct Run<V> {
    from: Cell<V>,
    to: Cell<V>,
    /// The latest target composed.
    target: Cell<V>,
    /// How many times the target changed: what starts a run.
    changes: Cell<u32>,
}

/// `target`, or on its way there from the last one on `spec`, `lerp` taking the two ends and how
/// far along. Read in composition, as `animate_*_as_state`'s value read there, so each of its
/// frames composes the caller. A new target starts from what shows, with no speed: within a single
/// animatable, the velocity of an interrupted run has nothing to carry over to.
pub(crate) fn animate_lazily<V: Copy + PartialEq + 'static>(
    target: V,
    spec: AnimationSpec,
    lerp: fn(V, V, f32) -> V,
) -> V {
    let run = remember(|| {
        Rc::new(Run {
            from: Cell::new(target),
            to: Cell::new(target),
            target: Cell::new(target),
            changes: Cell::new(0),
        })
    });
    if run.target.get() != target {
        run.target.set(target);
        run.changes.set(run.changes.get() + 1);
    }
    let changes = run.changes.get();
    key(changes > 0, || {
        if changes == 0 {
            return target;
        }
        let progress = remember(|| Animatable::new(1.0f32));
        LaunchedEffect(changes, {
            let (run, progress) = (run.clone(), progress.clone());
            async move {
                run.from.set(between(&run, progress.value(), lerp));
                run.to.set(target);
                progress.snap_to(0.0);
                progress.animate_to(1.0, spec).await;
            }
        });
        between(&run, progress.value(), lerp)
    })
}

fn between<V: Copy>(run: &Run<V>, fraction: f32, lerp: fn(V, V, f32) -> V) -> V {
    if fraction == 1.0 { run.to.get() } else { lerp(run.from.get(), run.to.get(), fraction) }
}
