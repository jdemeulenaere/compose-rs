//! Twelfth review round: `AnimatedVisibility` interrupted mid-enter, against Jetpack Compose's
//! `Transition` interruption handling. Each test states the input, what Compose does, and what
//! happens instead.

#![allow(non_snake_case)]

use std::time::Duration;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Color, UnitExt, platform::DrawOp};

fn alphas(ui: &mut ComposeTestRule) -> Vec<f32> {
    ui.host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::SaveLayerAlpha { alpha, .. } => Some(*alpha),
            _ => None,
        })
        .collect()
}

/// Input: `AnimatedVisibility` entering with `fadeIn(tween(100, LinearEasing)) +
/// scaleIn(tween(1000), 0.5)` and leaving with `fadeOut(tween(400, LinearEasing)) +
/// scaleOut(tween(1000), 0.5)`. It's shown, and hidden 300 ms later (the fade-in is over, the
/// scale-in still running); its alpha is read 100 ms after that.
///
/// Compose: each part is its own `TransitionAnimationState`, and only an animation that hasn't
/// finished is interrupted (`updateAnimation(isInterrupted = !isFinished)`) and takes the
/// interruption spring. The alpha finished its fade-in, so it fades out along its own 400 ms
/// linear tween: 0.75 after 100 ms. (The scale, interrupted, springs.)
///
/// Here: an exit launched while any part is running respecs every part, so the finished fade
/// also takes the interruption spring (stiffness 1500, critically damped): its alpha is about
/// 0.1 after 100 ms, the fade-out collapsing four times faster than asked.
#[test]
fn an_exit_after_a_finished_part_keeps_that_parts_spec() {
    let visible = mutable_state_of(false);
    let handle = visible.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        AnimatedVisibility!(
            handle.get(),
            enter = fade_in_with(tween(100).easing(LINEAR_EASING), 0.0)
                + scale_in_with(tween(1000), 0.5, compose_ui::TransformOrigin::CENTER),
            exit = fade_out_with(tween(400).easing(LINEAR_EASING), 0.0)
                + scale_out_with(tween(1000), 0.5, compose_ui::TransformOrigin::CENTER),
            {
                Box!(
                    modifier = Modifier
                        .size((50.dp(), 20.dp()))
                        .background(Color::BLACK)
                        .test_tag("panel")
                );
            }
        );
    });
    visible.set(true);
    ui.wait_for_idle();
    ui.advance_time_by(Duration::from_millis(300));
    let shown = alphas(&mut ui);
    assert!(shown.iter().all(|alpha| *alpha > 0.99), "faded in: {shown:?}");
    visible.set(false);
    ui.wait_for_idle();
    ui.advance_time_by(Duration::from_millis(100));
    let leaving = alphas(&mut ui);
    assert!(
        leaving.iter().any(|alpha| (*alpha - 0.75).abs() < 0.06),
        "100 ms into a 400 ms linear fade-out, the alpha should be about 0.75: {leaving:?}"
    );
}
