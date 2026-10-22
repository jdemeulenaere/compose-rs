//! `Crossfade` and `Modifier.animate_content_size` through the test rule's clock.

#![allow(non_snake_case)]

use std::time::Duration;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{UnitExt, platform::DrawOp};

fn alphas(ui: &mut ComposeTestRule) -> Vec<f32> {
    ui.host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::SaveLayerAlpha(alpha) => Some(*alpha),
            _ => None,
        })
        .collect()
}

#[test]
fn crossfade_shows_both_while_fading_then_only_the_new_one() {
    #[composable]
    fn Screen(page: MutableState<u32>) {
        Crossfade!(page.get(), animation_spec = tween(100).easing(LINEAR_EASING), |page| {
            BasicText!("page {}", page);
        });
    }
    let page = mutable_state_of(1u32);
    let handle = page.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || Screen(handle));
    ui.on_node_with_text("page 1").assert_exists();
    page.set(2);
    ui.wait_for_idle();
    ui.advance_time_by(Duration::from_millis(48));
    ui.on_node_with_text("page 1").assert_exists();
    ui.on_node_with_text("page 2").assert_exists();
    let fading = alphas(&mut ui);
    assert_eq!(fading.len(), 2, "both pages in layers: {fading:?}");
    ui.advance_time_by(Duration::from_millis(200));
    ui.on_node_with_text("page 1").assert_does_not_exist();
    ui.on_node_with_text("page 2").assert_exists();
}

#[test]
fn a_content_size_animates_to_its_new_size() {
    #[composable]
    fn Expanding(expanded: MutableState<bool>) {
        Column!({
            Box!(
                modifier = Modifier.animate_content_size_with(tween(100).easing(LINEAR_EASING)),
                {
                    Spacer(
                        Modifier.size((50.dp(), if expanded.get() { 100.dp() } else { 20.dp() })),
                    );
                }
            );
            BasicText!("below", modifier = Modifier.test_tag("below"));
        });
    }
    let expanded = mutable_state_of(false);
    let handle = expanded.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || Expanding(handle));
    let top =
        |ui: &mut ComposeTestRule| ui.on_node_with_tag("below").fetch_semantics_node().bounds.top;
    assert_eq!(top(&mut ui), 20.0);
    expanded.set(true);
    ui.wait_for_idle();
    assert_eq!(top(&mut ui), 20.0, "starts where it was");
    ui.advance_time_by(Duration::from_millis(64));
    let midway = top(&mut ui);
    assert!(midway > 20.0 && midway < 100.0, "{midway}");
    ui.advance_time_by(Duration::from_millis(100));
    assert_eq!(top(&mut ui), 100.0);
}
