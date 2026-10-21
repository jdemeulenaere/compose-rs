//! Ninth review round: robustness of the animation APIs with NaN targets and states. Each test
//! states the input, what Compose does (or what a robust library should do), and what happens
//! instead.

#![allow(non_snake_case)]

use compose_animation::*;
use compose_foundation::BasicText;
use compose_runtime::executor;
use compose_test::*;

/// `animateFloatAsState(done / total)` with `total` 0: the target is `0f / 0f` (NaN), as a
/// progress bar's animated fraction is before any work is known. Compose compares the new
/// target with the animatable's as boxed floats (`equals`: NaN equals NaN), so nothing animates
/// and the UI is idle. Here `animate_value_as_state` compares targets with `==` (NaN never
/// equals itself), so every pass launches an animation to NaN; the spring's rest check never
/// holds for NaN, and each NaN frame is a state change that recomposes and relaunches it. The
/// UI never goes idle: the test rule gives up after a minute of test time (3,752 frames), and an
/// app redraws at 60 fps forever.
#[test]
fn a_nan_animation_target_lets_the_ui_go_idle() {
    let mut ui = compose_test(|| {
        let (done, total) = (0.0f32, 0.0f32);
        let fraction = animate_float_as_state!(done / total);
        BasicText!("{}", fraction.get());
    });
    ui.wait_for_idle();
}

/// The same with a tween, `animateFloatAsState(Float.NaN, tween(100))`, which ends by its
/// duration whatever its values: Compose never starts it (the target equals the current one).
/// Here it ends after 100 ms, but each pass sees a "new" target (`NaN != NaN`) and launches it
/// again, and its NaN frames are each a change (`mutable_state_of`'s `PartialEq` policy) that
/// recomposes: the UI never goes idle either.
#[test]
fn a_nan_tween_target_ends_with_its_duration() {
    let mut ui = compose_test(|| {
        let value = animate_float_as_state!(f32::NAN, animation_spec = tween(100));
        BasicText!("{}", value.get());
    });
    ui.wait_for_idle();
}

/// A screen's state with a float that is NaN while unknown, `data class Reading(val label:
/// String, val value: Float)`, shown with `Crossfade(Reading("Speed", Float.NaN))`. Kotlin's
/// data-class `equals` compares floats with `Float.compare` (NaN equals NaN), so Compose's
/// `Crossfade` sees the same state each pass and shows one content. Here states are told apart
/// with `PartialEq`, and a derived `PartialEq` never finds the NaN field equal: every pass adds
/// the "new" state as another item, launches a fade for each one kept, and asks for another
/// pass. After 30 frames the tree holds 64 nodes (a content per pass, where Compose has one);
/// items and fade tasks keep growing every frame (a `wait_for_idle` on this content didn't
/// return in 10 minutes).
#[test]
fn a_crossfade_target_with_a_nan_field_is_one_state() {
    #[derive(Clone, PartialEq, Debug)]
    struct Reading {
        label: &'static str,
        value: f32,
    }
    executor::set_manual_clock(executor::now_nanos());
    let mut host = compose_ui::UiHost::headless(200, 200);
    host.set_content(|| {
        Crossfade!(Reading { label: "Speed", value: f32::NAN }, |reading: Reading| {
            BasicText!("{} {}", reading.label, reading.value);
        });
    });
    for _ in 0..30 {
        executor::advance_clock(FRAME_DURATION);
        host.frame();
    }
    let nodes = host.with_tree(|tree| tree.node_count());
    assert!(nodes < 10, "{nodes} nodes after 30 frames: a content per pass");
}
