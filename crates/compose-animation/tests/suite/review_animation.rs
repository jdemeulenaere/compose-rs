//! Animation review: failing tests for compose-animation (docs/06-ui.md, "Animation"; D120).
//! Each test states the input, what Jetpack Compose does, and what happens instead.
//!
//! Checked and sound (no test kept): the spring equations against Compose's
//! `SpringSimulation.updateValues` (under-, critically and over-damped); the cubic Bézier
//! easings and their constants; tween fractions with delays, zero durations and snaps; an
//! Oklab round trip; `animate*AsState` retargeting from the current value and velocity, and
//! its finished listener (interrupted animations don't call it); animations end, and nothing
//! waits for a frame once they have; fade and crossfade frames recompose and re-measure
//! nothing, and `animate_content_size` frames don't re-measure its content; the Skia replay of
//! `Transform` (pivot, scale, rotation, translation) and `SaveLayerAlpha` (group opacity over
//! overlapping children), rendered to pixels.

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
            DrawOp::SaveLayerAlpha { alpha, .. } => Some(*alpha),
            _ => None,
        })
        .collect()
}

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
    let ui = ComposeTestRule::with_manual_clock(move || Panel(handle, enter, exit));
    (ui, visible)
}

fn below_top(ui: &mut ComposeTestRule) -> f32 {
    ui.on_node_with_tag("below").fetch_semantics_node().bounds.top
}

/// A critically damped spring from 0 to 1 at `stiffness`, `seconds` in, from rest: Compose's
/// `SpringSimulation` with `dampingRatio = 1`.
fn critically_damped(stiffness: f32, seconds: f32) -> f32 {
    let natural = stiffness.sqrt();
    1.0 - (1.0 + natural * seconds) * (-natural * seconds).exp()
}

// ---- AnimatedVisibility ---------------------------------------------------------------------

/// Show, and hide again three frames later, while the content is still fading in (a toggle
/// tapped twice, a hover that ends quickly). Compose's transition turns around: the content
/// fades out from where it is and leaves when the exit ends. Here the content vanishes on the
/// next frame: the interrupted enter's tasks end (`Interrupted`), each counts itself down, and
/// the last one sees `launched == false` (the exit is now the visibility launched) and clears
/// `leaving`, although the exit's own animations have only just started. The same happens to
/// any exit that interrupts an enter, and to hide, show, hide.
///
/// Severity: high. Visible to anyone toggling quickly: the exit animation is skipped and the
/// content pops out.
#[test]
fn hiding_during_the_enter_plays_the_exit() {
    let (mut ui, visible) = panel(fade_in(), fade_out());
    visible.set(true);
    ui.wait_for_idle();
    for _ in 0..3 {
        ui.advance_time_by_frame();
    }
    let entering = alphas(&mut ui);
    assert!(entering.iter().any(|alpha| *alpha > 0.0 && *alpha < 1.0), "{entering:?}");
    visible.set(false);
    ui.wait_for_idle();
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    // The exit (a spring from about 0.35 to 0) is far from over.
    ui.on_node_with_text("details").assert_exists();
    let leaving = alphas(&mut ui);
    assert!(leaving.iter().any(|alpha| *alpha > 0.0 && *alpha < 1.0), "{leaving:?}");
}

/// Enter `fade_in() + expand_vertically()`, exit `fade_out()` (no shrink). Shown fully, then
/// hidden, then shown again while fading out. In Compose the size is the transition's
/// `changeSize` (the enter's or the exit's): the exit has none, so its target stays the full
/// size, and coming back leaves the size where it is, 40 px. Here the exit snaps the size
/// progress, which it doesn't animate, to 0; the enter then expands from 0, so the content
/// collapses to nothing on the frame it is shown again and pushes what follows up and back
/// down. (The alpha does the same with an enter that has no fade: it jumps to 1.)
///
/// Severity: medium. A visible jump on reversal whenever enter and exit animate different
/// parts, a common pairing (`expandVertically` in, `fadeOut` out).
#[test]
fn showing_again_keeps_a_size_the_exit_didnt_change() {
    let (mut ui, visible) = panel(fade_in() + expand_vertically(), fade_out());
    ui.set_auto_advance(true);
    visible.set(true);
    ui.wait_for_idle();
    assert_eq!(below_top(&mut ui), 40.0);
    ui.set_auto_advance(false);
    visible.set(false);
    ui.wait_for_idle();
    for _ in 0..3 {
        ui.advance_time_by_frame();
    }
    assert_eq!(below_top(&mut ui), 40.0, "the fade out keeps the size");
    visible.set(true);
    ui.wait_for_idle();
    let mut tops = vec![below_top(&mut ui)];
    for _ in 0..3 {
        ui.advance_time_by_frame();
        tops.push(below_top(&mut ui));
    }
    assert!(tops.iter().all(|top| *top == 40.0), "collapsed and re-expanded: {tops:?}");
}

/// `fade_in()`'s default spec. Compose's `fadeIn(animationSpec = spring(stiffness =
/// Spring.StiffnessMediumLow))` (and every enter and exit default: `expandIn`, `slideIn`,
/// `scaleIn`, `shrinkOut`, …) uses stiffness 400. Here `default_spec()` is `spring()`, stiffness
/// 1500: 96 ms in, the alpha is 0.885 where Compose's is 0.572, and every default enter and
/// exit runs about twice as fast as Compose's (sqrt(1500 / 400) = 1.94).
///
/// Severity: medium-low. A numerical difference in every default transition.
#[test]
fn fade_in_defaults_to_a_medium_low_spring() {
    let (mut ui, visible) = panel(fade_in(), fade_out());
    visible.set(true);
    ui.wait_for_idle();
    // The first frame starts the animation (play time 0); six more are 96 ms in.
    for _ in 0..7 {
        ui.advance_time_by_frame();
    }
    let alpha = alphas(&mut ui)[0];
    let compose = critically_damped(Spring::STIFFNESS_MEDIUM_LOW, 0.096);
    assert!((alpha - compose).abs() < 0.02, "alpha {alpha} at 96 ms, Compose's {compose}");
}

// ---- Modifier.animate_content_size ----------------------------------------------------------

/// `animate_content_size()`'s default spec. Compose's is `spring(stiffness =
/// Spring.StiffnessMediumLow, visibilityThreshold = IntSize.VisibilityThreshold)`; here it's
/// `spring()` with a 1 px threshold, stiffness 1500: a 20 to 100 px growth is at about 91 px
/// 96 ms in, where Compose's is at about 66 px.
///
/// Severity: medium-low. A numerical difference in every default `animateContentSize`.
#[test]
fn animate_content_size_defaults_to_a_medium_low_spring() {
    #[composable]
    fn Expanding(expanded: MutableState<bool>) {
        Column!({
            Box!(modifier = Modifier.animate_content_size(), {
                Spacer(Modifier.size((50.dp(), if expanded.get() { 100.dp() } else { 20.dp() })));
            });
            BasicText!("below", modifier = Modifier.test_tag("below"));
        });
    }
    let expanded = mutable_state_of(false);
    let handle = expanded.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || Expanding(handle));
    expanded.set(true);
    ui.wait_for_idle();
    for _ in 0..7 {
        ui.advance_time_by_frame();
    }
    let top = below_top(&mut ui);
    let compose = 20.0 + 80.0 * critically_damped(Spring::STIFFNESS_MEDIUM_LOW, 0.096);
    assert!((top - compose).abs() < 2.0, "{top} px at 96 ms, Compose's {compose}");
}

// ---- Repeatable, reversed -------------------------------------------------------------------

/// Samples an animatable from 0 to `target` along `spec`: the value at each frame, the first
/// at play time 0, then every 16 ms.
fn samples(target: f32, spec: AnimationSpec, frames: usize) -> Vec<f32> {
    let mut ui = ComposeTestRule::with_manual_clock(|| {});
    let animatable = Animatable::new(0.0f32);
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
    values
}

/// `repeatable(2, tween(100), RepeatMode::Reverse)` from 0 to 100 (the default easing,
/// `FastOutSlowInEasing`). Compose's `VectorizedRepeatableSpec` reverses time: an odd iteration
/// plays the tween at `(count + 1) * duration - playTime`, so at 128 ms the value is the tween's
/// at 72 ms, 100 * FastOutSlowIn(0.72), about 95. Here the odd iteration runs the tween forward
/// with its ends swapped, 100 * (1 - FastOutSlowIn(0.28)), about 69: with any easing that
/// isn't point-symmetric the way back follows another curve (fast where Compose's is slow), so
/// a pulsing `infinite_repeatable(tween(…), Reverse)` looks different from Compose's.
///
/// Severity: medium. A numerical difference in every reversed repeat with the default easing.
#[test]
fn a_reversed_repetition_plays_the_tween_backwards_in_time() {
    let values = samples(100.0, repeatable(2, tween(100), RepeatMode::Reverse).into(), 9);
    // values[i] is at play time 16 * i ms.
    let at_128 = values[8];
    let compose = 100.0 * FAST_OUT_SLOW_IN_EASING.transform(0.72);
    assert!((at_128 - compose).abs() < 1.0, "{at_128} at 128 ms, Compose's {compose}: {values:?}");
}

/// The same root cause with a delay: `infinite_repeatable(tween(100).delay(50), Reverse)`,
/// linear. Compose reverses the whole iteration, delay included, so the delay of a reversed
/// iteration comes at its end: the value turns around at the target at once (at 160 ms it's
/// the tween's at 140 ms, 90) and waits 100 ms at the start between iterations. Here the
/// reversed iteration waits its delay at the target first (100 from 150 to 200 ms).
///
/// Severity: low (same fix as above).
#[test]
fn a_reversed_repetition_with_a_delay_waits_at_the_start_only() {
    let spec = infinite_repeatable(tween(100).delay(50).easing(LINEAR_EASING), RepeatMode::Reverse);
    let values = samples(100.0, spec.into(), 11);
    let at_160 = values[10];
    assert!((at_160 - 90.0).abs() < 1.0, "{at_160} at 160 ms, Compose's 90: {values:?}");
}

// ---- Crossfade ------------------------------------------------------------------------------

/// Crossfade from page 1 to page 2 and back to page 1 before the first fade ends. Compose's
/// `Crossfade` keeps one content per state (`currentlyVisible` replaces the entry with the same
/// `contentKey`): page 1's content, with its remembered state, fades back in from where it
/// is. Here the return pushes a new item for page 1 (its state fresh, its alpha from 0) while
/// the old page 1 keeps fading out: two page 1 contents are composed at once, and the one that
/// stays has lost its remembered state (a scroll position, a text field's text).
///
/// Severity: medium. Duplicate content and lost state on a quick back-and-forth (a tab
/// switched and switched back).
#[test]
fn crossfading_back_reuses_the_contents_of_the_state() {
    #[composable]
    fn Screen(page: MutableState<u32>) {
        Crossfade!(page.get(), animation_spec = tween(100).easing(LINEAR_EASING), |page| {
            BasicText!("page {}", page);
        });
    }
    let page = mutable_state_of(1u32);
    let handle = page.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || Screen(handle));
    page.set(2);
    ui.wait_for_idle();
    ui.advance_time_by(Duration::from_millis(48));
    page.set(1);
    ui.wait_for_idle();
    ui.advance_time_by_frame();
    assert_eq!(ui.count_nodes(&has_text("page 1")), 1, "page 1 composed twice");
}

// ---- animate*AsState ------------------------------------------------------------------------

thread_local! {
    static HEARD: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// The finished listener given on the latest composition. Compose's `animateValueAsState`
/// holds it in `rememberUpdatedState(finishedListener)` and calls the current one when an
/// animation ends. Here the task launched for the target keeps the listener of the pass that
/// launched it: a listener recomposed meanwhile (one capturing newer state, say the item it
/// reports for) isn't the one called.
///
/// Severity: low. A stale callback, a difference from Compose that is hard to notice.
#[test]
fn the_finished_listener_is_the_latest_one() {
    #[composable]
    fn Tagged(target: MutableState<f32>, tag: MutableState<u32>) {
        let tag = tag.get();
        let value = animate_float_as_state!(
            target.get(),
            animation_spec = tween(100),
            finished_listener = move |_| HEARD.with(|heard| heard.set(tag)),
        );
        BasicText!("{:.1}", value.get());
    }
    let target = mutable_state_of(0.0f32);
    let tag = mutable_state_of(1u32);
    let (target_handle, tag_handle) = (target.handle(), tag.handle());
    let mut ui = ComposeTestRule::with_manual_clock(move || Tagged(target_handle, tag_handle));
    target.set(1.0);
    ui.wait_for_idle();
    ui.advance_time_by(Duration::from_millis(48));
    tag.set(2);
    ui.wait_for_idle();
    ui.advance_time_by(Duration::from_millis(200));
    ui.on_node_with_text("1.0").assert_exists();
    assert_eq!(HEARD.with(std::cell::Cell::get), 2, "the listener of the first pass was called");
}
