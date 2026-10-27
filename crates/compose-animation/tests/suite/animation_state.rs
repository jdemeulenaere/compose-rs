//! `AnimationState::animate_to` and `animate`, as animation-core's `SuspendAnimation.kt`: the
//! first frame at play time 0, each frame the `TargetBasedAnimation`'s, the end velocity the
//! animation's (D871).

use std::cell::RefCell;
use std::rc::Rc;

use compose_animation::*;
use compose_runtime::*;
use compose_test::*;

/// Runs `animate_to(100, tween(100, linear))` from 0, recording each frame's value and
/// velocity, and the state it leaves.
fn linear_tween() -> (Vec<(f32, f32)>, AnimationState<f32>) {
    let mut ui = compose_test(|| {});
    let frames: Rc<RefCell<Vec<(f32, f32)>>> = Rc::default();
    let left: Rc<RefCell<Option<AnimationState<f32>>>> = Rc::default();
    let (into, out) = (frames.clone(), left.clone());
    CoroutineScope::new().launch(async move {
        let mut state = AnimationState::new(0.0f32, 0.0);
        state
            .animate_to(100.0, tween(100).easing(LINEAR_EASING), false, |scope| {
                into.borrow_mut().push((scope.value(), scope.velocity()));
            })
            .await;
        *out.borrow_mut() = Some(state);
    });
    ui.wait_for_idle();
    let state = left.borrow_mut().take().expect("the animation finished");
    (frames.take(), state)
}

// Review of D854: Compose's `animate` starts with a frame at play time 0, the initial value.
#[test]
fn the_first_frame_is_at_play_time_zero() {
    let (frames, _) = linear_tween();
    assert_eq!(frames[0].0, 0.0, "{frames:?}");
}

// Review of D854: a finished animation leaves the animation's end velocity (a linear tween's,
// 1,000 per second here), not 0, as Compose's `doAnimationFrame`.
#[test]
fn a_finished_tween_leaves_its_end_velocity() {
    let (frames, state) = linear_tween();
    let (last_value, last_velocity) = *frames.last().unwrap();
    assert_eq!(last_value, 100.0);
    assert!((last_velocity - 1000.0).abs() < 1.0, "{last_velocity}");
    assert!((state.velocity() - 1000.0).abs() < 1.0, "{}", state.velocity());
    assert!(!state.is_running());
    assert_ne!(state.finished_time_nanos(), 0);
}
