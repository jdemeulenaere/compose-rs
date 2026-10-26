//! Adversarial review of D434: material components in lazy items reused from the kept pool.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::UnitExt;

/// Items leave in one pass (deactivated: `on_reset` snaps the switch to its *old* value then),
/// and come back in a later pass with the other value. The reused switch must show its value
/// at once, as a new one (D431, D434), not animate from the old item's value. The `reset` flag
/// that made `update` snap after a deactivation was removed by 17ce11a.
#[test]
fn a_switch_reused_from_the_kept_pool_snaps_to_its_new_value() {
    let count = mutable_state_of(6usize);
    let flip = mutable_state_of(false);
    let (count_handle, flip_handle) = (count.handle(), flip.handle());
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        LazyColumn!(modifier = Modifier.size((200.dp(), 400.dp())), move |scope: LazyListScope| {
            let flipped = flip_handle.get();
            scope.items(count_handle.get(), move |index| {
                Box!(modifier = Modifier.height(40.dp()), {
                    Switch!((index % 2 == 0) != flipped, Some(|_| {}));
                });
            })
        });
    });
    ui.wait_for_idle();
    ui.advance_time_by(std::time::Duration::from_secs(2));
    // All items leave: deactivated and kept.
    count.set(0);
    ui.wait_for_idle();
    ui.advance_time_by(std::time::Duration::from_secs(2));
    // They come back in their slots (same keys), each with the other value.
    flip.set(true);
    count.set(6);
    ui.wait_for_idle();
    // The frame that recomposes for it: Compose recomposes only on frames (D900).
    ui.advance_time_by_frame();
    let now = ui.dump_display_list();
    ui.advance_time_by(std::time::Duration::from_secs(2));
    let settled = ui.dump_display_list();
    assert_eq!(now, settled, "the reused switches animated from the old items' values");
}

/// The same for radio buttons.
#[test]
fn a_radio_button_reused_from_the_kept_pool_snaps_to_its_new_value() {
    let count = mutable_state_of(6usize);
    let flip = mutable_state_of(false);
    let (count_handle, flip_handle) = (count.handle(), flip.handle());
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        LazyColumn!(modifier = Modifier.size((200.dp(), 400.dp())), move |scope: LazyListScope| {
            let flipped = flip_handle.get();
            scope.items(count_handle.get(), move |index| {
                Box!(modifier = Modifier.height(40.dp()), {
                    RadioButton!((index % 2 == 0) != flipped, Some(|| {}));
                });
            })
        });
    });
    ui.wait_for_idle();
    ui.advance_time_by(std::time::Duration::from_secs(2));
    count.set(0);
    ui.wait_for_idle();
    ui.advance_time_by(std::time::Duration::from_secs(2));
    flip.set(true);
    count.set(6);
    ui.wait_for_idle();
    // The frame that recomposes for it: Compose recomposes only on frames (D900).
    ui.advance_time_by_frame();
    let now = ui.dump_display_list();
    ui.advance_time_by(std::time::Duration::from_secs(2));
    let settled = ui.dump_display_list();
    assert_eq!(now, settled, "the reused radio buttons animated from the old items' values");
}

/// Control: a fresh composition with the flipped values shows what the settled frame shows.
#[test]
fn control_switches_settle_to_the_same_frame() {
    let flip = mutable_state_of(false);
    let flip_handle = flip.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        LazyColumn!(modifier = Modifier.size((200.dp(), 400.dp())), move |scope: LazyListScope| {
            let flipped = flip_handle.get();
            scope.items(6, move |index| {
                Box!(modifier = Modifier.height(40.dp()), {
                    Switch!((index % 2 == 0) != flipped, Some(|_| {}));
                });
            })
        });
    });
    ui.wait_for_idle();
    let first = ui.dump_display_list();
    ui.advance_time_by(std::time::Duration::from_secs(2));
    assert_eq!(first, ui.dump_display_list());
}

/// Plain scrolling: the composed range grows and shrinks in different passes, so items come
/// from the kept pool (deactivated earlier), not only from the items leaving in the same pass.
#[test]
fn switches_scrolled_into_a_list_show_their_values_at_once() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        LazyColumn!(
            modifier = Modifier.size((200.dp(), 200.dp())),
            state = given,
            |scope: LazyListScope| scope.items(1000, |index| {
                Box!(modifier = Modifier.height(40.dp()), {
                    Switch!(index % 3 == 0, Some(|_| {}));
                });
            })
        );
    });
    ui.wait_for_idle();
    ui.advance_time_by(std::time::Duration::from_secs(2));
    let mut mismatches = Vec::new();
    for step in 0..60 {
        state.dispatch_raw_delta(40.0);
        ui.wait_for_idle();
        let now = ui.dump_display_list();
        ui.advance_time_by(std::time::Duration::from_secs(2));
        if now != ui.dump_display_list() {
            mismatches.push(step);
        }
    }
    assert!(mismatches.is_empty(), "frames showing animating reused switches: {mismatches:?}");
}

/// Prefetch (between frames) composes the next items into slots kept for reuse, deactivated in
/// an earlier pass: each switch whose value differs from the old item's starts an animation,
/// offscreen, which asks for frames ("no frame is asked for", says `UiHost::prefetch`).
#[test]
fn prefetching_reused_switches_starts_no_animation() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::with_manual_clock(move || {
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
    ui.advance_time_by(std::time::Duration::from_secs(2));
    let mut animating = Vec::new();
    for step in 0..40 {
        state.dispatch_raw_delta(40.0);
        ui.wait_for_idle();
        ui.advance_time_by(std::time::Duration::from_secs(2));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while ui.host().prefetch(deadline) {}
        if compose_runtime::platform::has_frame_awaiters() || ui.host().needs_frame() {
            animating.push(step);
        }
        ui.advance_time_by(std::time::Duration::from_secs(2));
    }
    assert!(animating.is_empty(), "prefetches that started animations: {animating:?}");
}

/// Control: the same with every switch on (a reused one keeps its value): prefetch asks for
/// no frame.
#[test]
fn control_prefetching_same_valued_switches_asks_for_no_frame() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        LazyColumn!(
            modifier = Modifier.size((200.dp(), 200.dp())),
            state = given,
            |scope: LazyListScope| scope.items(1000, |_| {
                Box!(modifier = Modifier.height(40.dp()), {
                    Switch!(true, Some(|_| {}));
                });
            })
        );
    });
    ui.wait_for_idle();
    ui.advance_time_by(std::time::Duration::from_secs(2));
    let mut animating = Vec::new();
    for step in 0..40 {
        state.dispatch_raw_delta(40.0);
        ui.wait_for_idle();
        ui.advance_time_by(std::time::Duration::from_secs(2));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while ui.host().prefetch(deadline) {}
        if compose_runtime::platform::has_frame_awaiters() || ui.host().needs_frame() {
            animating.push(step);
        }
        ui.advance_time_by(std::time::Duration::from_secs(2));
    }
    assert!(animating.is_empty(), "prefetches that asked for frames: {animating:?}");
}

/// Items reused in the pass their old items leave (a jump): each switch whose value differs
/// gets an animation launched by `update` (17ce11a removed the `reset` flag that made it snap)
/// and snapped by `on_reset` after; the task launched still runs and waits for a frame.
#[test]
fn a_jump_reusing_switches_of_other_values_leaves_no_animation_running() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::with_manual_clock(move || {
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
    ui.advance_time_by(std::time::Duration::from_secs(2));
    // Every slot reused by an item of the other value.
    state.request_scroll_to_item(100, 0);
    ui.wait_for_idle();
    let awaiting = compose_runtime::platform::has_frame_awaiters();
    assert!(!awaiting, "reused switches left animations waiting for frames");
}

/// A switch toggled in place still animates: its animation starts when it's drawn (D435), and
/// only a reset (reuse) shows a new value at once.
#[test]
fn a_toggled_switch_still_animates() {
    let checked = mutable_state_of(false);
    let handle = checked.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        Box!(modifier = Modifier.size((200.dp(), 100.dp())), {
            Switch!(handle.get(), Some(|_| {}));
        });
    });
    ui.wait_for_idle();
    let off = ui.dump_display_list();
    checked.set(true);
    ui.wait_for_idle();
    ui.advance_time_by(std::time::Duration::from_millis(16));
    let moving = ui.dump_display_list();
    ui.advance_time_by(std::time::Duration::from_secs(2));
    let on = ui.dump_display_list();
    assert_ne!(moving, on, "the thumb jumped to its new place");
    assert_ne!(off, on);
}
