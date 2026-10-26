//! Eleventh review round: `AnimatedContent` with states coming back while they leave, against
//! Jetpack Compose's `AnimatedContent` (`AnimatedEnterExit` on a child `Transition`). Each test
//! states the input, what Compose does, and what happens instead.

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

/// Input: `AnimatedContent(count)` whose contents fade in after 90 ms
/// (`fadeIn(tween(300, delayMillis = 90, LinearEasing))`) as the old one fades out
/// (`fadeOut(tween(300, LinearEasing))`), no size transform. The count goes 0 -> 1 and, 144 ms
/// in (content 0 at about half alpha), back to 0; 160 ms later content 0's alpha is read.
///
/// Compose: each content is an `AnimatedEnterExit` on a child transition of `EnterExitState`.
/// Content 0 was heading for `PostExit`; coming back makes its segment `PostExit -> Visible`,
/// which is neither the enter's (`PreEnter -> Visible`) nor the exit's, so its alpha animates
/// along `DefaultAlphaAndScaleSpring` (and an interrupted animation takes a spring anyway, as
/// `interruptionSpec`): it springs back from half alpha at once, past 0.9 after 160 ms.
///
/// Here: the content coming back replays its enter from where it is, delay included: content 0
/// stays at half alpha for 90 ms, then crawls up along the 300 ms tween (about 0.6 after 160
/// ms). Toggling a state back and forth leaves the content dimmed instead of recovering.
#[test]
fn a_content_coming_back_while_it_leaves_springs_back_at_once() {
    let count = mutable_state_of(0);
    let handle = count.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        AnimatedContent!(
            handle.get(),
            transition_spec = |_, _| {
                fade_in_with(tween(300).delay(90).easing(LINEAR_EASING), 0.0)
                    .together_with(fade_out_with(tween(300).easing(LINEAR_EASING), 0.0))
                    .without_size_transform()
            },
            |value: i32| {
                Box!(
                    modifier = Modifier
                        .size((50.dp(), 20.dp()))
                        .background(Color::BLACK)
                        .test_tag(format!("{value}"))
                );
            }
        );
    });
    count.set(1);
    ui.wait_for_idle();
    ui.advance_time_by(Duration::from_millis(144));
    let leaving = alphas(&mut ui);
    assert!(
        leaving.iter().any(|alpha| *alpha > 0.35 && *alpha < 0.65),
        "content 0 about half faded out: {leaving:?}"
    );
    count.set(0);
    ui.wait_for_idle();
    ui.advance_time_by(Duration::from_millis(160));
    ui.on_node_with_tag("0").assert_exists();
    let back = alphas(&mut ui);
    let highest = back.iter().copied().fold(0.0f32, f32::max);
    assert!(
        highest > 0.85,
        "content 0 should have sprung back (alpha > 0.85) 160 ms after coming back; the \
         layers' alphas: {back:?}"
    );
}
