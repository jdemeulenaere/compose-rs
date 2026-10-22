//! Adversarial review of retained drawing per repaint boundary (docs/06-ui.md, "Repaint
//! boundaries"): each animation frame draws what drawing everything again at that moment does.

#![allow(non_snake_case)]

use std::time::Duration;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Color, UnitExt};

/// The display list the last frame drew, and the one drawing every boundary again draws now:
/// they're the same unless a boundary kept a stale drawing.
fn retained_and_redrawn(ui: &mut ComposeTestRule) -> (String, String) {
    let retained = ui.host().display_list().dump();
    let redrawn = ui.host().with_tree(|tree| {
        tree.invalidate_draw();
        tree.draw();
        tree.display_list().dump()
    });
    (retained, redrawn)
}

fn check_frames(ui: &mut ComposeTestRule, frames: usize, what: &str) {
    for frame in 0..frames {
        ui.advance_time_by_frame();
        let (retained, redrawn) = retained_and_redrawn(ui);
        assert_eq!(retained, redrawn, "{what}, frame {frame}");
    }
}

#[composable]
fn Scene(page: MutableState<u32>, visible: MutableState<bool>) {
    Column!(modifier = Modifier.repaint_boundary(), {
        Box!(modifier = Modifier.repaint_boundary().size_wh(60.dp(), 20.dp()), {
            Crossfade!(page.get(), animation_spec = tween(100).easing(LINEAR_EASING), |page| {
                Box!(
                    modifier = Modifier
                        .background(Color::from_argb(0xFF00_0000 | (page * 0x40)))
                        .size_wh(60.dp(), 20.dp())
                );
            });
        });
        // Faded and scaled: a layer lifted out of its content.
        AnimatedVisibility!(
            visible.get(),
            enter = fade_in() + scale_in(0.5),
            exit = fade_out() + scale_out(0.5),
            {
                Box!(
                    modifier = Modifier
                        .repaint_boundary()
                        .background(Color::RED)
                        .size_wh(40.dp(), 10.dp())
                );
            }
        );
        // Expanded: clipped, its layer inside its content.
        AnimatedVisibility!(
            visible.get(),
            enter = fade_in() + expand_vertically(),
            exit = fade_out() + shrink_vertically(),
            {
                Box!(modifier = Modifier.background(Color::GREEN).size_wh(40.dp(), 30.dp()));
            }
        );
        Box!(modifier = Modifier.repaint_boundary().background(Color::BLUE).size(8.dp()));
    });
}

#[test]
fn crossfade_and_visibility_frames_draw_what_drawing_everything_does() {
    let page = mutable_state_of(1u32);
    let visible = mutable_state_of(false);
    let (page_handle, visible_handle) = (page.handle(), visible.handle());
    let mut ui = ComposeTestRule::with_manual_clock(move || Scene(page_handle, visible_handle));
    page.set(2);
    check_frames(&mut ui, 12, "crossfading to 2");
    visible.set(true);
    check_frames(&mut ui, 30, "showing");
    page.set(3);
    visible.set(false);
    check_frames(&mut ui, 30, "crossfading to 3, hiding");
    // Interrupted midway.
    visible.set(true);
    check_frames(&mut ui, 4, "showing again");
    visible.set(false);
    check_frames(&mut ui, 30, "hiding again");
    // Settled: what a fresh host with the final values draws.
    ui.wait_for_idle();
    let fresh_page = mutable_state_of(3u32);
    let fresh_visible = mutable_state_of(false);
    let (page_handle, visible_handle) = (fresh_page.handle(), fresh_visible.handle());
    let mut fresh = ComposeTestRule::with_manual_clock(move || Scene(page_handle, visible_handle));
    fresh.wait_for_idle();
    assert_eq!(ui.dump_display_list(), fresh.dump_display_list());
    let _ = Duration::ZERO;
}
