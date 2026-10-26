//! The veil, `unveil_in` and `veil_out`, against Compose's `AnimatedVisibilityTest`: its color
//! frame by frame, through interruptions, a veil taken away mid-way, and a direction change
//! that resets the exits accumulated. The color is read as Compose's tests do, from the
//! content's transition's animations (`Built-in veil`).

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Color, UnitExt};

/// A color's channels as fractions, as Compose's `Color.red` and the like.
fn channels(color: Color) -> [f32; 4] {
    [color.red(), color.green(), color.blue(), color.alpha()].map(|channel| channel as f32 / 255.0)
}

fn assert_color(expected: [f32; 4], actual: Color, at: &str) {
    let actual = channels(actual);
    for (expected, actual) in expected.iter().zip(&actual) {
        assert!((expected - actual).abs() <= 0.02, "{at}: {actual:?}, Compose's {expected:?}");
    }
}

/// What the content saw of its transition's veil.
#[derive(Clone)]
struct Seen {
    color: Rc<Cell<Color>>,
    has_veil: Rc<Cell<bool>>,
    disposed: Rc<Cell<bool>>,
}

impl Seen {
    fn new() -> Seen {
        Seen {
            color: Rc::new(Cell::new(Color::TRANSPARENT)),
            has_veil: Rc::new(Cell::new(false)),
            disposed: Rc::new(Cell::new(false)),
        }
    }
}

/// An `AnimatedVisibility` of a 100 dp box whose content reads its transition's veil.
fn veiled(
    visible: MutableState<bool>,
    enter: MutableState<EnterTransition>,
    exit: MutableState<ExitTransition>,
    seen: Seen,
) -> ComposeTestRule {
    ComposeTestRule::with_manual_clock(move || {
        let seen = seen.clone();
        AnimatedVisibility!(
            visible.get(),
            enter = enter.get(),
            exit = exit.get(),
            move |scope: AnimatedVisibilityScope| {
                let seen = seen.clone();
                Box!(modifier = Modifier.required_size(100.dp()), {
                    let veil = scope
                        .transition()
                        .animations()
                        .into_iter()
                        .find(|animation| animation.label().contains("veil"));
                    seen.has_veil.set(veil.is_some());
                    if let Some(color) = veil.and_then(|veil| veil.value::<Color>()) {
                        seen.color.set(color);
                    }
                    let disposed = seen.disposed.clone();
                    DisposableEffect((), move |effect| {
                        let disposed = disposed.clone();
                        effect.on_dispose(move || disposed.set(true))
                    });
                });
            }
        );
    })
}

fn linear_tween() -> TweenSpec {
    tween(160).easing(LINEAR_EASING)
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedVisibilityTest.kt, animateVisibilitySlideAndVeilTest
#[test]
fn a_veil_out_darkens_frame_by_frame() {
    let visible = mutable_state_of(true);
    let target = Color::RED;
    let enter = mutable_state_of(EnterTransition::NONE);
    let exit =
        mutable_state_of(veil_out_with(tween(160).easing(FAST_OUT_SLOW_IN_EASING), target, false));
    let seen = Seen::new();
    let mut ui = veiled(visible.handle(), enter.handle(), exit.handle(), seen.clone());
    visible.set(false);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    for play in (0..=160).step_by(16) {
        let fraction = FAST_OUT_SLOW_IN_EASING.transform(play as f32 / 160.0);
        // Red throughout, its alpha the fraction (Compose's `lerp` of the two in Oklab).
        assert_color([1.0, 0.0, 0.0, fraction], seen.color.get(), &format!("at {play} ms"));
        ui.advance_time_by(Duration::from_millis(16));
        ui.wait_for_idle();
    }
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedVisibilityTest.kt, animateVisibilityVeilInterruptionEnterToExitTest
#[test]
fn an_unveil_interrupted_by_a_veil_out_goes_on_from_where_it_was() {
    let visible = mutable_state_of(false);
    let (enter_color, exit_color) = (Color::RED, Color::BLUE);
    let enter = mutable_state_of(unveil_in_with(linear_tween(), enter_color, false));
    let exit = mutable_state_of(veil_out_with(linear_tween(), exit_color, false));
    let seen = Seen::new();
    let mut ui = veiled(visible.handle(), enter.handle(), exit.handle(), seen.clone());
    visible.set(true);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    // Half-way through the enter.
    ui.advance_time_by(Duration::from_millis(80));
    ui.wait_for_idle();
    let interrupted = seen.color.get();
    assert_color([1.0, 0.0, 0.0, 0.5], interrupted, "half-way in");
    visible.set(false);
    // A few frames along the interruption's spring (a spring keeps its momentum, so more
    // than one).
    for _ in 0..3 {
        ui.advance_time_by_frame();
        ui.wait_for_idle();
    }
    // From red to blue, the alpha growing again.
    let [red, _, blue, alpha] = channels(seen.color.get());
    let [interrupted_red, _, interrupted_blue, interrupted_alpha] = channels(interrupted);
    assert!(red < interrupted_red, "{:?}", seen.color.get());
    assert!(blue > interrupted_blue, "{:?}", seen.color.get());
    assert!(alpha > interrupted_alpha, "{:?}", seen.color.get());
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedVisibilityTest.kt, animateVisibilityVeilInterruptionRemoveVeilTest
#[test]
fn a_veil_taken_away_mid_way_still_clears() {
    let visible = mutable_state_of(false);
    let (enter_color, exit_color) = (Color::RED, Color::BLUE);
    let enter = mutable_state_of(unveil_in_with(linear_tween(), enter_color, false));
    let exit = mutable_state_of(veil_out_with(linear_tween(), exit_color, false));
    let seen = Seen::new();
    let mut ui = veiled(visible.handle(), enter.handle(), exit.handle(), seen.clone());
    visible.set(true);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(80));
    ui.wait_for_idle();
    let interrupted = seen.color.get();
    assert_color([1.0, 0.0, 0.0, 0.5], interrupted, "half-way in");
    // No veil any more.
    exit.set(ExitTransition::NONE);
    enter.set(EnterTransition::NONE);
    for _ in 0..3 {
        ui.advance_time_by_frame();
        ui.wait_for_idle();
    }
    // The veil goes on clearing, though the transitions no longer have it.
    let [red, green, blue, alpha] = channels(seen.color.get());
    assert_color([1.0, 0.0, 0.0, alpha], seen.color.get(), "red still");
    assert!(red > 0.98 && green < 0.02 && blue < 0.02);
    assert!(alpha < channels(interrupted)[3], "{:?}", seen.color.get());
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedVisibilityTest.kt, animateVisibilityVeilNoInterruptionTest
#[test]
fn a_veil_clears_in_then_darkens_out() {
    let visible = mutable_state_of(false);
    let (enter_color, exit_color) = (Color::RED, Color::BLUE);
    let enter = mutable_state_of(unveil_in_with(linear_tween(), enter_color, false));
    let exit = mutable_state_of(veil_out_with(linear_tween(), exit_color, false));
    let seen = Seen::new();
    let mut ui = veiled(visible.handle(), enter.handle(), exit.handle(), seen.clone());
    visible.set(true);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(160));
    ui.wait_for_idle();
    // Clear once in.
    assert_color([1.0, 0.0, 0.0, 0.0], seen.color.get(), "in");
    visible.set(false);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(160));
    ui.wait_for_idle();
    // Blue by the end, then gone (its last alpha unseen, the content disposed).
    let [red, green, blue, _] = channels(seen.color.get());
    assert!(red < 0.02 && green < 0.02 && blue > 0.98, "{:?}", seen.color.get());
    assert!(seen.disposed.get());
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedVisibilityTest.kt, verifyDirectionChangeResetsAccumulatedTransitions
#[test]
fn a_direction_change_clears_a_veil_the_new_exit_lacks() {
    let visible = mutable_state_of(true);
    let enter = mutable_state_of(EnterTransition::NONE);
    let exit = mutable_state_of(veil_out_with(linear_tween(), Color::BLUE, false));
    let seen = Seen::new();
    let mut ui = veiled(visible.handle(), enter.handle(), exit.handle(), seen.clone());
    // Exiting...
    visible.set(false);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(80));
    ui.wait_for_idle();
    // ...back in...
    visible.set(true);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(40));
    ui.wait_for_idle();
    // ...and out again, with an exit without a veil.
    exit.set(fade_out());
    visible.set(false);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(80));
    ui.wait_for_idle();
    // The exit is the last one's neutralized (its veil going back to clear) and the fade: the
    // veil stays to clear smoothly.
    assert!(seen.has_veil.get(), "the veil animation kept to clear");
    let before = seen.color.get();
    ui.advance_time_by(Duration::from_millis(80));
    ui.wait_for_idle();
    let after = seen.color.get();
    assert!(channels(after)[3] < channels(before)[3], "{before:?} to {after:?}");
}
