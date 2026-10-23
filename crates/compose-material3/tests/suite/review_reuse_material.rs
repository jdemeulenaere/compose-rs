//! Adversarial review of D430-D433: material components in reused lazy items.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_material3::*;
use compose_test::*;
use compose_ui::{IntSize, UnitExt};

/// Switches of alternating values: an item reusing one of the other value. `SwitchNode::on_reset`
/// (called from `Applier::reuse`, during the pass) and its `update` after a reset `snap_to` the
/// `Animatable`, whose value is an `OwnedState`: a state write during composition, which debug
/// builds forbid.
#[test]
fn switches_in_reused_items_snap_without_writing_state_while_composing() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        LazyColumn!(
            modifier = Modifier.size((200.dp(), 200.dp())),
            state = given,
            |scope: LazyListScope| scope.items(1000, |index| {
                Box!(modifier = Modifier.height(40.dp()), {
                    Switch!(index % 2 == 0, Some(|_| {}));
                });
            })
        );
    });
    ui.wait_for_idle();
    for _ in 0..20 {
        state.scroll_by(40.0);
        ui.wait_for_idle();
    }
}

/// The same for radio buttons.
#[test]
fn radio_buttons_in_reused_items_snap_without_writing_state_while_composing() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        LazyColumn!(
            modifier = Modifier.size((200.dp(), 200.dp())),
            state = given,
            |scope: LazyListScope| scope.items(1000, |index| {
                Box!(modifier = Modifier.height(40.dp()), {
                    RadioButton!(index % 2 == 0, Some(|| {}));
                });
            })
        );
    });
    ui.wait_for_idle();
    for _ in 0..20 {
        state.scroll_by(40.0);
        ui.wait_for_idle();
    }
}
