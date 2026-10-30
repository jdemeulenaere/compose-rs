//! `AnimatedVisibility`: content animating in and out through the test rule's clock.

#![allow(non_snake_case)]

use std::time::Duration;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{UnitExt, platform::DrawOp};

#[composable]
fn Panel(visible: MutableState<bool>, enter: EnterTransition, exit: ExitTransition) {
    Column!({
        AnimatedVisibility!(visible.get(), enter = enter, exit = exit, {
            Box!(modifier = Modifier.size((100.dp(), 40.dp())).test_tag("panel"), {
                BasicText!("details");
            });
        });
        BasicText!("below", modifier = Modifier.test_tag("below"));
    });
}

fn panel(enter: EnterTransition, exit: ExitTransition) -> (ComposeTestRule, OwnedState<bool>) {
    let visible = mutable_state_of(false);
    let handle = visible.handle();
    let ui = ComposeTestRule::with_manual_clock(move || Panel(handle, enter.clone(), exit.clone()));
    (ui, visible)
}

fn below_top(ui: &mut ComposeTestRule) -> f32 {
    ui.on_node_with_tag("below").fetch_semantics_snapshot().bounds.top
}

#[test]
fn hidden_content_isnt_composed_and_takes_no_space() {
    let (mut ui, _) = panel(expand_vertically(), shrink_vertically());
    ui.on_node_with_text("details").assert_does_not_exist();
    assert_eq!(below_top(&mut ui), 0.0);
}

#[test]
fn expanding_pushes_what_follows_down_frame_by_frame() {
    let (mut ui, visible) = panel(
        EnterTransition::NONE + expand_vertically(),
        ExitTransition::NONE + shrink_vertically(),
    );
    visible.set(true);
    ui.wait_for_idle();
    // The frame that recomposes for the new target: Compose recomposes only on frames (D900).
    ui.advance_time_by_frame();
    ui.on_node_with_text("details").assert_exists();
    let mut tops = Vec::new();
    for _ in 0..40 {
        ui.advance_time_by_frame();
        tops.push(below_top(&mut ui));
    }
    assert!(tops.windows(2).all(|pair| pair[0] <= pair[1]), "{tops:?}");
    assert!(tops[2] > 0.0 && tops[2] < 40.0, "{tops:?}");
    assert_eq!(*tops.last().unwrap(), 40.0);
}

#[test]
fn leaving_content_stays_until_its_exit_ends() {
    let (mut ui, visible) = panel(fade_in(), fade_out());
    visible.set(true);
    ui.wait_for_idle();
    ui.advance_time_by(Duration::from_secs(1));
    visible.set(false);
    ui.wait_for_idle();
    // The frame that recomposes for the new target: Compose recomposes only on frames (D900).
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.on_node_with_text("details").assert_exists();
    // Fading: drawn in a layer with an alpha between 0 and 1.
    let alphas: Vec<f32> = ui
        .host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::SaveLayerAlpha { alpha, .. } => Some(*alpha),
            _ => None,
        })
        .collect();
    assert!(alphas.iter().any(|alpha| *alpha > 0.0 && *alpha < 1.0), "{alphas:?}");
    ui.advance_time_by(Duration::from_secs(1));
    ui.on_node_with_text("details").assert_does_not_exist();
}

#[test]
fn showing_again_while_leaving_comes_back_from_where_it_is() {
    let (mut ui, visible) = panel(expand_vertically(), shrink_vertically());
    ui.set_auto_advance(true);
    visible.set(true);
    ui.wait_for_idle();
    assert_eq!(below_top(&mut ui), 40.0);
    ui.set_auto_advance(false);
    visible.set(false);
    ui.wait_for_idle();
    for _ in 0..4 {
        ui.advance_time_by_frame();
    }
    let midway = below_top(&mut ui);
    assert!(midway > 0.0 && midway < 40.0, "{midway}");
    visible.set(true);
    ui.wait_for_idle();
    ui.advance_time_by_frame();
    let after = below_top(&mut ui);
    assert!((after - midway).abs() < 10.0, "jumped from {midway} to {after}");
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_eq!(below_top(&mut ui), 40.0);
    ui.on_node_with_text("details").assert_exists();
}

#[test]
fn an_animation_frame_neither_recomposes_nor_remeasures_the_rest() {
    let (mut ui, visible) = panel(fade_in(), fade_out());
    visible.set(true);
    ui.wait_for_idle();
    // Its start's frames: what the transition wrote and read is heard at the apply (D793).
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    let before = ui.recomposition_counts();
    let stats = {
        ui.advance_time_by_frame();
        ui.recomposition_counts().since(&before)
    };
    assert_eq!(stats.names(), Vec::new(), "a fade frame recomposed");
}
