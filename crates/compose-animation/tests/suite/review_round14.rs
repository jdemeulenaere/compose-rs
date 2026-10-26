//! Review round 14: `keyframes` against Compose's `KeyframesSpec` (`VectorizedKeyframesSpec`)
//! as `Animatable` (`TargetBasedAnimation`) and the repeatables play it. Each test states the
//! input, what Compose does, and what happens here.
//!
//! Checked and sound (no test kept): values between keyframes with each stretch's easing, the
//! start and target standing in at 0 and the duration, keyframes at 0 overriding the start,
//! the delay, keyframes past the duration ignored, a later keyframe at the same time replacing
//! an earlier, multi-component values (`Offset`, `Color` in Oklab), reversed repetitions
//! (time reversed, as `VectorizedRepeatableSpec`), interruptions (a transition's spring,
//! `animate*AsState` restarting from the current value), equal specs sharing an entry, the
//! velocity reset once an animation ends (as `Animatable.endAnimation`).
//!
//! Not kept, as D150 documents it: each distinct keyframes spec stays in the thread's table
//! for good (keyframes built from changing data grow it, each `keyframes` call scanning it).

use compose_animation::*;
use compose_runtime::*;
use compose_test::*;

/// Runs `animatable` along `spec` to `target` in a test rule, returning the value at each
/// frame: the first at play time 0, then every 16 ms.
fn samples(
    animatable: &Animatable<f32>,
    target: f32,
    spec: AnimationSpec,
    frames: usize,
) -> (ComposeTestRule, Vec<f32>) {
    let mut ui = ComposeTestRule::with_manual_clock(|| {});
    let scope = CoroutineScope::new();
    let running = animatable.clone();
    scope.launch(async move {
        running.animate_to(target, spec).await;
    });
    let mut values = Vec::new();
    for _ in 0..frames {
        ui.advance_time_by_frame();
        values.push(animatable.value());
    }
    drop(scope);
    (ui, values)
}

/// `repeatable(3, keyframes(100, …), RepeatMode::Restart)` from 0 to 100 (an attention
/// shake played three times). Compose's `TargetBasedAnimation` returns the target once the
/// play time reaches the repeatable's duration (`isFinishedFromNanos`: `targetValue`), so the
/// value ends at 100. Here the last frame evaluates the repeatable at its total duration,
/// which `Restart` maps to the start of a fourth iteration (`play − count × duration` = 0):
/// the value ends at the start, 0, and stays there; an `animate_float_as_state` with this
/// spec settles away from its target, and its finished listener hears 0. (The same with a
/// `tween`, and with `Reverse` and an even count.)
///
/// Severity: high. A finite repeatable never ends at its target.
#[test]
fn a_finite_repeatable_of_keyframes_ends_at_its_target() {
    let spec = keyframes(|frames| {
        frames.duration_millis = 100;
        frames.at(50, 30.0f32);
    });
    let animatable = Animatable::new(0.0f32);
    let (_ui, values) =
        samples(&animatable, 100.0, repeatable(3, spec, RepeatMode::Restart).into(), 30);
    assert_eq!(animatable.value(), 100.0, "{values:?}");
}

/// A keyframes spec whose last keyframe, at the duration, isn't the target
/// (`keyframes(100) { at(50, 120.0); at(100, 90.0) }` animated to 100: an overshoot drawn by
/// hand that ends a little short). Compose's `TargetBasedAnimation` returns the target once
/// finished, so `animate*AsState` ends at 100 (`value == targetValue` after every finished
/// animation, which callers rely on). Here the last frame is the keyframe's 90: the state
/// rests at 90 with a target of 100, and the finished listener hears 90.
///
/// Severity: low. A keyframe at the duration that disagrees with the target is unusual.
#[test]
fn a_keyframes_animation_ends_at_its_target() {
    let spec = keyframes(|frames| {
        frames.duration_millis = 100;
        frames.at(50, 120.0f32);
        frames.at(100, 90.0f32);
    });
    let animatable = Animatable::new(0.0f32);
    let (_ui, values) = samples(&animatable, 100.0, spec.into(), 10);
    assert_eq!(animatable.value(), 100.0, "{values:?}");
}

/// Lock-in: a `KeyframesSpec`'s duration and delay are read-only (its keyframes are interned,
/// D150): what it reports is what plays.
#[test]
fn a_keyframes_specs_duration_is_the_one_played() {
    let spec = keyframes(|frames| {
        frames.duration_millis = 200;
        frames.at(0, 0.0f32);
    });
    assert_eq!((spec.duration_millis(), spec.delay_millis()), (200, 0));
    let animatable = Animatable::new(0.0f32);
    let (_ui, values) = samples(&animatable, 100.0, spec.into(), 8);
    // values[i] is at 16 × i ms: 112 ms in, half-way along 200 ms (linearly), not done.
    assert!((values[7] - 56.0).abs() < 1.0, "{} at 112 ms: {values:?}", values[7]);
}
