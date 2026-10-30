//! `DeferredAnimatedVisibility` against Compose's `DeferredAnimatedVisibilityTest`: a pending
//! target composes the content but holds its enter or exit; a `MutableTransform` moves it by hand,
//! and the animation takes up from there, at its velocity. Compose's tests run with a real clock
//! for `testTimeSource`; here the velocities use the rule's clock.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::DrawOp;
use compose_ui::{Color, IntOffset, IntSize, Modifier, Offset, TransformOrigin, UnitExt};

fn millis(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

fn linear(duration: u32) -> TweenSpec {
    tween(duration).easing(LINEAR_EASING)
}

fn close(actual: f32, expected: f32, tolerance: f32, what: &str) {
    assert!((actual - expected).abs() <= tolerance, "{what}: {actual}, Compose's {expected}");
}

/// A deferred visibility's rule and what its content's box reports.
struct Fixture {
    ui: ComposeTestRule,
    state: DeferredTransitionState<bool>,
    x: Rc<Cell<f32>>,
    composed: Rc<Cell<bool>>,
    outer_size: Rc<Cell<IntSize>>,
}

/// A box of 100 in a deferred visibility.
fn fixture(
    initial: bool,
    enter: Option<EnterTransition>,
    exit: Option<ExitTransition>,
    transform: impl Fn(&DeferredTransitionState<bool>) -> Option<MutableTransform> + 'static,
) -> Fixture {
    let state = DeferredTransitionState::new(initial);
    let x = Rc::new(Cell::new(0.0f32));
    let composed = Rc::new(Cell::new(false));
    let outer_size = Rc::new(Cell::new(IntSize::ZERO));
    let (held, seen_x, seen_composed, seen_size) =
        (state.clone(), x.clone(), composed.clone(), outer_size.clone());
    let ui = compose_test(move || {
        let transition = remember_deferred_transition(&held, None);
        let (seen_x, seen_composed) = (seen_x.clone(), seen_composed.clone());
        let seen_size = seen_size.clone();
        let transform = {
            let held = held.clone();
            let made = remember(|| Rc::new(transform(&held)));
            (*made).clone()
        };
        transition.DeferredAnimatedVisibility(
            |visible| *visible,
            Modifier.on_globally_positioned(move |coordinates: compose_ui::LayoutCoordinates| {
                seen_size.set(coordinates.size())
            }),
            enter.clone(),
            exit.clone(),
            transform,
            move |_| {
                let seen_x = seen_x.clone();
                let composed = seen_composed.clone();
                DisposableEffect(0, move |effect| {
                    composed.set(true);
                    let composed = composed.clone();
                    effect.on_dispose(move || composed.set(false))
                });
                Box!(
                    modifier = Modifier.size(100.dp()).test_tag("content").on_globally_positioned(
                        move |coordinates: compose_ui::LayoutCoordinates| {
                            seen_x.set(coordinates.position_in_root().x);
                        }
                    )
                );
            },
        );
    });
    Fixture { ui, state, x, composed, outer_size }
}

/// The box's width in the root in pixels, its layer's scale counted: read from the tree, as
/// `onGloballyPositioned` isn't called for a layer's change alone (Compose's is, D828).
fn width(ui: &mut ComposeTestRule) -> f32 {
    let bounds = ui.on_node_with_tag("content").get_bounds_in_root();
    ui.density().to_px(bounds.right - bounds.left)
}

/// The box's width in the frame before it leaves, moving the clock until it has.
fn last_width(ui: &mut ComposeTestRule) -> f32 {
    let mut last = 0.0;
    while !ui.on_all_nodes(has_test_tag("content")).fetch_semantics_nodes().is_empty() {
        last = width(ui);
        ui.advance_time_by_frame();
    }
    last
}

/// A transform setting the scale while something is pending.
fn scale_while_pending(
    preview: MutableState<f32>,
) -> impl Fn(&DeferredTransitionState<bool>) -> Option<MutableTransform> + 'static {
    move |state| {
        let state = state.clone();
        Some(MutableTransform::new(move |scope, _| {
            if state.pending_target_state().is_some() {
                scope.set_scale(preview.get());
            }
        }))
    }
}

/// The same setting the offset.
fn offset_while_pending(
    preview: MutableState<i32>,
) -> impl Fn(&DeferredTransitionState<bool>) -> Option<MutableTransform> + 'static {
    move |state| {
        let state = state.clone();
        Some(
            MutableTransform::new(move |scope, _| {
                if state.pending_target_state().is_some() {
                    scope.set_offset(IntOffset::new(preview.get(), 0));
                }
            })
            .offset_velocity_provider(|| Offset::ZERO),
        )
    }
}

fn fade_in_linear(duration: u32) -> Option<EnterTransition> {
    Some(fade_in_with(linear(duration), 0.0))
}

fn fade_out_linear(duration: u32) -> Option<ExitTransition> {
    Some(fade_out_with(linear(duration), 0.0))
}

fn scale_in_linear(duration: u32) -> Option<EnterTransition> {
    Some(scale_in_with(linear(duration), 0.0, TransformOrigin::CENTER))
}

fn scale_out_linear(duration: u32) -> Option<ExitTransition> {
    Some(scale_out_with(linear(duration), 0.0, TransformOrigin::CENTER))
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedVisibilityTest.kt, visibility_enterGateClosed_defersAnimationUntilReady
#[test]
fn a_pending_enter_composes_the_content_and_holds_it_until_animated_to() {
    let Fixture { mut ui, state, composed, .. } =
        fixture(false, fade_in_linear(100), fade_out_linear(100), |_| None);
    ui.on_node_with_tag("content").assert_does_not_exist();
    assert!(!composed.get());
    state.defer(true);
    ui.wait_for_idle();
    assert!(composed.get(), "composed at once, to be prepared");
    ui.on_node_with_tag("content").assert_exists();
    state.animate_to(true);
    ui.advance_time_by(millis(50));
    ui.on_node_with_tag("content").assert_exists();
    ui.advance_time_by(millis(200));
    ui.on_node_with_tag("content").assert_is_displayed();
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedVisibilityTest.kt, visibility_exitGateClosed_holdsContentInExitStateUntilReady
#[test]
fn a_pending_exit_keeps_the_content_until_animated_to() {
    let Fixture { mut ui, state, composed, .. } =
        fixture(true, fade_in_linear(100), fade_out_linear(100), |_| None);
    ui.wait_for_idle();
    assert!(composed.get());
    state.defer(false);
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    ui.advance_time_by(millis(500));
    assert!(composed.get(), "kept while the exit is pending");
    ui.on_node_with_tag("content").assert_is_displayed();
    state.animate_to(false);
    ui.advance_time_by(millis(200));
    ui.wait_for_idle();
    assert!(!composed.get(), "gone once the exit ends");
    ui.on_node_with_tag("content").assert_does_not_exist();
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedVisibilityTest.kt, visibility_enterGateFlapping_cannotBeClosedOnceStarted
#[test]
fn an_enter_started_cannot_be_held_again() {
    let Fixture { mut ui, state, .. } = fixture(false, fade_in_linear(1000), None, |_| None);
    state.defer(true);
    ui.wait_for_idle();
    state.animate_to(true);
    ui.advance_time_by(millis(100));
    state.defer(true);
    ui.advance_time_by(millis(1000));
    ui.on_node_with_tag("content").assert_is_displayed();
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedVisibilityTest.kt, visibility_exitGateFlapping_cannotBeClosedOnceStarted
#[test]
fn an_exit_started_cannot_be_held_again() {
    let Fixture { mut ui, state, .. } = fixture(true, None, fade_out_linear(100), |_| None);
    state.defer(false);
    ui.wait_for_idle();
    state.animate_to(false);
    ui.advance_time_by(millis(100));
    state.defer(false);
    ui.advance_time_by(millis(1000));
    ui.on_node_with_tag("content").assert_does_not_exist();
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedVisibilityTest.kt, transition_childState_remainsInPreEnterDuringPreparation
#[test]
fn the_content_stays_pre_enter_while_prepared() {
    let state = DeferredTransitionState::new(false);
    let current = Rc::new(Cell::new(None));
    let (held, seen) = (state.clone(), current.clone());
    let mut ui = compose_test(move || {
        let transition = remember_deferred_transition(&held, None);
        let seen = seen.clone();
        transition.DeferredAnimatedVisibility(
            |visible| *visible,
            Modifier,
            None,
            None,
            None,
            move |scope| {
                seen.set(Some(scope.transition().current_state()));
                Box!(modifier = Modifier.size(100.dp()));
            },
        );
    });
    state.defer(true);
    ui.wait_for_idle();
    assert_eq!(current.get(), Some(EnterExitState::PreEnter));
    state.animate_to(true);
    ui.advance_time_by(millis(50));
    assert_eq!(current.get(), Some(EnterExitState::PreEnter));
    ui.advance_time_by(millis(1000));
    ui.wait_for_idle();
    assert_eq!(current.get(), Some(EnterExitState::Visible));
}

fn expand_from_nothing(duration: u32) -> Option<EnterTransition> {
    Some(expand_in_with(linear(duration), compose_ui::Alignment::BOTTOM_END, true, |_| {
        IntSize::ZERO
    }))
}

fn shrink_to_nothing(duration: u32) -> Option<ExitTransition> {
    Some(shrink_out_with(linear(duration), compose_ui::Alignment::BOTTOM_END, true, |_| {
        IntSize::ZERO
    }))
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedVisibilityTest.kt, layout_size_isHeldAtInitialStateDuringPreparation
#[test]
fn the_size_is_held_at_the_enter_s_start_while_prepared() {
    let Fixture { mut ui, state, outer_size, .. } =
        fixture(false, expand_from_nothing(100), None, |_| None);
    state.defer(true);
    ui.wait_for_idle();
    assert_eq!(outer_size.get(), IntSize::ZERO);
    state.animate_to(true);
    ui.set_auto_advance(false);
    ui.wait_for_idle();
    ui.advance_time_by(millis(50));
    assert!(outer_size.get().width > 0 && outer_size.get().height > 0, "{:?}", outer_size.get());
    ui.advance_time_by(millis(100));
    let full = ui.density().round_to_px(100.dp());
    assert_eq!(outer_size.get(), IntSize::new(full, full));
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedVisibilityTest.kt, interruption_isDeferred_whenGateIsClosed
#[test]
fn an_interruption_waits_while_it_is_pending() {
    let Fixture { mut ui, state, composed, outer_size, .. } =
        fixture(false, expand_from_nothing(100), shrink_to_nothing(100), |_| None);
    state.animate_to(true);
    ui.set_auto_advance(false);
    ui.wait_for_idle();
    ui.advance_time_by(millis(50));
    let half = outer_size.get().width;
    assert!(half > 0);
    state.defer(false);
    ui.wait_for_idle();
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(20));
    assert!(outer_size.get().width > half, "{} goes on growing", outer_size.get().width);
    state.animate_to(false);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    let before = outer_size.get().width;
    ui.advance_time_by(millis(20));
    assert!(outer_size.get().width < before, "{} shrinks from {before}", outer_size.get().width);
    ui.advance_time_by(millis(200));
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    ui.on_node_with_tag("content").assert_does_not_exist();
    assert!(!composed.get());
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedVisibilityTest.kt, visibility_previewScaleSustained_whenNoScaleInTransitions
#[test]
fn a_manual_scale_stays_through_an_exit_without_one() {
    let preview = mutable_state_of(1.0f32);
    let Fixture { mut ui, state, .. } = fixture(
        true,
        fade_in_linear(160),
        fade_out_linear(160),
        scale_while_pending(preview.handle()),
    );
    ui.wait_for_idle();
    let full = width(&mut ui);
    assert!(full > 0.0);
    state.defer(false);
    ui.advance_time_by_frame();
    preview.set(0.5);
    ui.wait_for_idle();
    close(width(&mut ui), full * 0.5, 1.0, "the preview's");
    // The clock by hand from here: the box leaves with the exit, and is read from the tree. The
    // last seen is what Compose's test reads (its `waitForIdle` runs the exit to its end): the
    // manual scale's handed-off velocity has died out, the scale back at the value kept.
    ui.set_auto_advance(false);
    state.animate_to(state.pending_target_state().unwrap_or(state.target_state()));
    let last = last_width(&mut ui);
    close(last, full * 0.5, 5.0, "sustained");
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedVisibilityTest.kt, visibility_previewOffsetSustained_whenNoOffsetInTransitions
#[test]
fn a_manual_offset_stays_through_an_exit_without_one() {
    let preview = mutable_state_of(0);
    let Fixture { mut ui, state, x, .. } = fixture(
        true,
        fade_in_linear(160),
        fade_out_linear(160),
        offset_while_pending(preview.handle()),
    );
    ui.wait_for_idle();
    assert_eq!(x.get(), 0.0);
    state.defer(false);
    ui.advance_time_by_frame();
    preview.set(50);
    ui.wait_for_idle();
    assert_eq!(x.get(), 50.0);
    state.animate_to(state.pending_target_state().unwrap_or(state.target_state()));
    ui.advance_time_by(millis(80));
    ui.wait_for_idle();
    assert_eq!(x.get(), 50.0);
    ui.advance_time_by(millis(1000));
    ui.wait_for_idle();
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedVisibilityTest.kt, visibility_previewScale_handoffToExitTransition
#[test]
fn a_manual_scale_hands_off_to_the_exit() {
    let preview = mutable_state_of(1.0f32);
    let Fixture { mut ui, state, .. } = fixture(
        true,
        Some(scale_in_with(tween(160), 0.0, TransformOrigin::CENTER)),
        Some(scale_out_with(tween(160), 0.0, TransformOrigin::CENTER)),
        scale_while_pending(preview.handle()),
    );
    ui.wait_for_idle();
    let full = width(&mut ui);
    ui.set_auto_advance(false);
    state.defer(false);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    preview.set(0.5);
    ui.wait_for_idle();
    close(width(&mut ui), full * 0.5, 1.0, "the preview's");
    state.animate_to(false);
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(80));
    ui.advance_time_by_frame();
    let going = width(&mut ui);
    assert!(going < full * 0.5 && going > 0.0, "going to 0: {going}");
    ui.advance_time_by(millis(1000));
    ui.advance_time_by_frame();
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    ui.on_node_with_tag("content").assert_does_not_exist();
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedVisibilityTest.kt, visibility_previewOffset_handoffToEnterTransition
#[test]
fn a_manual_offset_hands_off_to_the_enter() {
    let preview = mutable_state_of(0);
    let Fixture { mut ui, state, x, .. } = fixture(
        false,
        Some(slide_in_horizontally_with(tween(160), |_| 200)),
        Some(slide_out_horizontally_with(tween(160), |_| 200)),
        offset_while_pending(preview.handle()),
    );
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    state.defer(true);
    ui.wait_for_idle();
    preview.set(100);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(x.get(), 100.0, "the preview's over the enter's start, 200");
    state.animate_to(true);
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(80));
    ui.advance_time_by_frame();
    assert!(x.get() < 100.0 && x.get() > 0.0, "going from 100 to 0: {}", x.get());
    ui.advance_time_by(millis(1000));
    ui.advance_time_by_frame();
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_eq!(x.get(), 0.0);
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedVisibilityTest.kt, visibility_midAnimation_updateTargetState_combinesTransformAndTransition
#[test]
fn a_deferred_phase_interrupting_an_animation_springs_to_the_manual_value() {
    let preview = mutable_state_of(1.0f32);
    let Fixture { mut ui, state, .. } = fixture(
        false,
        scale_in_linear(1000),
        scale_out_linear(1000),
        scale_while_pending(preview.handle()),
    );
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    state.animate_to(true);
    ui.wait_for_idle();
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(500));
    ui.wait_for_idle();
    let full = ui.density().round_to_px(100.dp()) as f32;
    close(width(&mut ui), full / 2.0, full * 0.1, "half way");
    preview.set(0.5);
    state.defer(false);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    let first = width(&mut ui);
    assert!(first < full * 0.55, "going to the manual 0.5: {first}");
    ui.advance_time_by_frame();
    let second = width(&mut ui);
    assert!(second <= first, "{second} then {first}");
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedVisibilityTest.kt, visibility_doublePreview_doesNotSnap
#[test]
fn a_second_preview_without_a_scale_does_not_snap_the_first_s() {
    let preview = mutable_state_of(1.0f32);
    let apply = mutable_state_of(true);
    let (applied, scale) = (apply.handle(), preview.handle());
    let Fixture { mut ui, state, .. } =
        fixture(true, fade_in_linear(160), fade_out_linear(160), move |state| {
            let state = state.clone();
            Some(MutableTransform::new(move |scope, _| {
                if state.pending_target_state().is_some() && applied.get() {
                    scope.set_scale(scale.get());
                }
            }))
        });
    ui.wait_for_idle();
    let full = width(&mut ui);
    ui.set_auto_advance(false);
    state.defer(false);
    preview.set(0.5);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    close(width(&mut ui), full * 0.5, 1.0, "the first preview's");
    state.animate_to(false);
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(80));
    ui.advance_time_by_frame();
    close(width(&mut ui), full * 0.5, 1.0, "sustained in the exit");
    apply.set(false);
    state.defer(true);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    let after_second = width(&mut ui);
    state.animate_to(true);
    ui.advance_time_by_frame();
    assert!(after_second < full * 0.9, "no snap: {after_second}");
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_eq!(width(&mut ui), full);
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedVisibilityTest.kt, visibility_previewScale_handoff_thenInterrupted_isSeamless
#[test]
fn an_exit_handed_off_then_interrupted_goes_on_from_where_it_is() {
    let preview = mutable_state_of(1.0f32);
    let Fixture { mut ui, state, .. } = fixture(
        true,
        scale_in_linear(1000),
        scale_out_linear(1000),
        scale_while_pending(preview.handle()),
    );
    ui.wait_for_idle();
    let full = width(&mut ui);
    ui.set_auto_advance(false);
    state.defer(false);
    preview.set(0.8);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    close(width(&mut ui), full * 0.8, 1.0, "the preview's");
    state.animate_to(false);
    ui.advance_time_by_frame();
    // From the 0.8 handed off to 0, half way.
    ui.advance_time_by(millis(500));
    ui.wait_for_idle();
    let before = width(&mut ui);
    close(before, full * 0.4, 5.0, "half way from 0.8");
    state.animate_to(true);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    let after = width(&mut ui);
    close(after, before, 5.0, "no snap back");
    ui.advance_time_by(millis(100));
    ui.wait_for_idle();
    let later = width(&mut ui);
    assert!(later > after, "growing: {later} from {after}");
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    close(width(&mut ui), full, 1.0, "at rest");
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedVisibilityTest.kt, visibility_previewScale_handoff_sustainUnlessSpecified_thenInterrupted_isSeamless
#[test]
fn a_sustained_manual_scale_interrupted_goes_back_without_a_snap() {
    let preview = mutable_state_of(1.0f32);
    let scale = preview.handle();
    let Fixture { mut ui, state, .. } =
        fixture(true, fade_in_linear(1000), fade_out_linear(1000), move |_| {
            Some(MutableTransform::new(move |scope, _| scope.set_scale(scale.get())))
        });
    ui.wait_for_idle();
    let full = width(&mut ui);
    ui.set_auto_advance(false);
    state.defer(false);
    preview.set(0.8);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    close(width(&mut ui), full * 0.8, 1.0, "the preview's");
    state.animate_to(false);
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(500));
    ui.wait_for_idle();
    close(width(&mut ui), full * 0.8, 1.0, "sustained");
    state.animate_to(true);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    let after = width(&mut ui);
    assert!(after < full * 0.95, "no snap: {after}");
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    close(width(&mut ui), full, 1.0, "at rest");
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedVisibilityTest.kt, visibility_previewScale_handoffUnspecifiedEnter_animatesToVisibleValue_thenInterrupted_isSeamless
#[test]
fn an_enter_without_a_scale_animates_the_manual_one_and_an_interruption_does_not_snap_it() {
    let preview = mutable_state_of(1.0f32);
    let scale = preview.handle();
    let Fixture { mut ui, state, .. } =
        fixture(false, fade_in_linear(1000), fade_out_linear(1000), move |_| {
            Some(MutableTransform::new(move |scope, _| scope.set_scale(scale.get())))
        });
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    state.defer(true);
    preview.set(0.8);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    let full = ui.density().to_px(100.dp());
    close(width(&mut ui), full * 0.8, 1.0, "composed at the preview's");
    state.animate_to(true);
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(50));
    ui.wait_for_idle();
    state.animate_to(false);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    let after = width(&mut ui);
    assert!(after < full * 0.95, "no snap: {after}");
    ui.advance_time_by(millis(100));
    ui.wait_for_idle();
    let later = width(&mut ui);
    assert!(later > after, "growing to 1: {later} from {after}");
    // It leaves once the exit ends: the last seen is near 1, a spring's threshold away.
    let last = last_width(&mut ui);
    close(last, full, 5.0, "the last before it left");
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedVisibilityTest.kt, visibility_previewScale_handoffVelocity
#[test]
fn a_faster_manual_scale_hands_off_a_greater_velocity() {
    let preview = mutable_state_of(1.0f32);
    let Fixture { mut ui, state, .. } = fixture(
        true,
        fade_in_linear(160),
        Some(scale_out_with(
            spring_with(Spring::DAMPING_RATIO_NO_BOUNCY, Spring::STIFFNESS_VERY_LOW),
            0.0,
            TransformOrigin::CENTER,
        )),
        scale_while_pending(preview.handle()),
    );
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    let gesture = |ui: &mut ComposeTestRule, steps: &[f32]| {
        state.defer(false);
        ui.advance_time_by_frame();
        ui.wait_for_idle();
        for step in steps {
            ui.advance_time_by_frame();
            preview.set(*step);
            ui.wait_for_idle();
        }
        let last = width(ui);
        state.animate_to(false);
        for _ in 0..3 {
            ui.advance_time_by_frame();
        }
        ui.wait_for_idle();
        last - width(ui)
    };
    let slow = gesture(&mut ui, &[0.9, 0.8, 0.7, 0.6, 0.5]);
    ui.set_auto_advance(true);
    preview.set(1.0);
    state.animate_to(true);
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    let fast = gesture(&mut ui, &[0.9, 0.7, 0.5]);
    assert!(fast > slow, "{fast} over {slow}");
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedVisibilityTest.kt, visibility_previewOffset_handoffVelocity
#[test]
fn the_offset_velocity_provider_is_what_the_animation_starts_at() {
    let velocity = mutable_state_of(Offset::ZERO);
    let read = velocity.handle();
    let Fixture { mut ui, state, x, .. } = fixture(
        true,
        fade_in_linear(160),
        Some(slide_out_horizontally_with(
            spring_with(Spring::DAMPING_RATIO_NO_BOUNCY, Spring::STIFFNESS_VERY_LOW),
            |width| width,
        )),
        move |state| {
            let state = state.clone();
            Some(
                MutableTransform::new(move |scope, _| {
                    if state.pending_target_state().is_some() {
                        scope.set_offset(IntOffset::new(100, 0));
                    }
                })
                .offset_velocity_provider(move || read.get()),
            )
        },
    );
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    let gesture = |ui: &mut ComposeTestRule| {
        state.defer(false);
        ui.advance_time_by_frame();
        ui.wait_for_idle();
        assert_eq!(x.get(), 100.0);
        state.animate_to(false);
        for _ in 0..3 {
            ui.advance_time_by_frame();
        }
        ui.wait_for_idle();
        x.get()
    };
    let without = gesture(&mut ui);
    ui.set_auto_advance(true);
    state.animate_to(true);
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    velocity.set(Offset::new(2000.0, 0.0));
    let with = gesture(&mut ui);
    assert!(with > without, "{with} over {without}");
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedVisibilityTest.kt, visibility_previewScale_interrupt_deferred_by_original_state_is_seamless
#[test]
fn animating_to_the_original_state_goes_back_from_the_manual_value() {
    let preview = mutable_state_of(1.0f32);
    let Fixture { mut ui, state, .. } = fixture(
        true,
        scale_in_linear(1000),
        scale_out_linear(1000),
        scale_while_pending(preview.handle()),
    );
    ui.wait_for_idle();
    let full = width(&mut ui);
    ui.set_auto_advance(false);
    state.defer(false);
    preview.set(0.8);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    close(width(&mut ui), full * 0.8, 1.0, "the preview's");
    state.animate_to(true);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    let after = width(&mut ui);
    close(after, full * 0.8, 1.0, "no jump");
    ui.advance_time_by(millis(100));
    ui.wait_for_idle();
    let later = width(&mut ui);
    assert!(later > after, "growing: {later} from {after}");
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    close(width(&mut ui), full, 1.0, "at rest");
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedVisibilityTest.kt, deferredAnimatedVisibility_interruptedEnter_doesNotFreezeVeil
#[test]
fn an_interrupted_enter_s_veil_goes_on_after_a_deferred_phase_not_touching_it() {
    let state = DeferredTransitionState::new(false);
    let active = mutable_state_of(false);
    let color = Rc::new(Cell::new(Color::TRANSPARENT));
    let (held, seen, handoff) = (state.clone(), color.clone(), active.handle());
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let transition = remember_deferred_transition(&held, None);
        let seen = seen.clone();
        let transform =
            handoff.get().then(|| MutableTransform::new(|scope, _| scope.set_scale(0.5)));
        transition.DeferredAnimatedVisibility(
            |visible| *visible,
            Modifier,
            unveil_in_with(linear(500), Color::BLACK, false),
            ExitTransition::NONE,
            transform,
            move |scope| {
                let veil = scope
                    .transition()
                    .animations()
                    .into_iter()
                    .find(|animation| animation.label().contains("veil"));
                if let Some(value) = veil.and_then(|veil| veil.value::<Color>()) {
                    seen.set(value);
                }
                Box!(modifier = Modifier.fill_max_size());
            },
        );
    });
    state.animate_to(true);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    ui.advance_time_by(millis(250));
    ui.wait_for_idle();
    active.set(true);
    state.defer(false);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    active.set(false);
    state.animate_to(false);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    let handed_off = color.get().alpha();
    ui.advance_time_by(millis(100));
    ui.wait_for_idle();
    assert!(handed_off > color.get().alpha(), "{handed_off} over {}", color.get().alpha());
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedVisibilityTest.kt, visibility_midAnimation_deferContinuesCatchUpWhenUnderlyingTransitionSettles
#[test]
fn the_catch_up_goes_on_when_the_animation_under_it_settles() {
    let Fixture { mut ui, state, .. } =
        fixture(false, scale_in_linear(100), scale_out_linear(100), |_| {
            Some(MutableTransform::new(|scope, _| scope.set_scale(0.9)))
        });
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    state.animate_to(true);
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(50));
    ui.wait_for_idle();
    let full = ui.density().round_to_px(100.dp()) as f32;
    close(width(&mut ui), full * 0.5, full * 0.1, "half way");
    state.defer(false);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    ui.advance_time_by(millis(32));
    ui.wait_for_idle();
    // The enter ends; the catch-up's spring doesn't.
    ui.advance_time_by(millis(32));
    ui.wait_for_idle();
    let after = width(&mut ui);
    assert!(after < full * 0.85, "no snap to 0.9: {after} of {full}");
}

// Compose: compose/animation/animation/src/commonMain/kotlin/androidx/compose/animation/AnimatedVisibility.kt, targetEnterExit (no test of Compose's: a pending target showing the content takes it back to PreEnter)
#[test]
fn a_gesture_back_during_an_exit_keeps_the_content_and_enters_it_from_its_enter_s_start() {
    let Fixture { mut ui, state, x, composed, .. } = fixture(
        true,
        Some(slide_in_horizontally_with(linear(100), |_| 200)),
        Some(slide_out_horizontally_with(linear(100), |_| -100)),
        |_| None,
    );
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    state.animate_to(false);
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(80));
    ui.advance_time_by_frame();
    close(x.get(), -80.0, 1.0, "the exit, 80 ms in");
    // Back to visible, held: the child transition's target is PreEnter, at +200 once the exit's
    // own frames take it there; the content stays however long it takes.
    state.defer(true);
    ui.advance_time_by(millis(1000));
    ui.wait_for_idle();
    assert!(composed.get(), "kept during the gesture");
    close(x.get(), 200.0, 1.0, "at the enter's start");
    // Released: it enters from there.
    state.animate_to(true);
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(48));
    assert!(x.get() < 200.0 && x.get() > 0.0, "entering: {}", x.get());
    ui.advance_time_by(millis(200));
    assert_eq!(x.get(), 0.0);
}

/// The offset's motion after a hand-off with no velocity provider, to an exit with no slide (the
/// hand-off keeps the last offset): the gesture's `steps`, a frame each, then where it is
/// three frames after the hand-off, from the last step.
fn overshoot_after(steps: &[i32]) -> f32 {
    let preview = mutable_state_of(0);
    let read = preview.handle();
    let Fixture { mut ui, state, x, .. } =
        fixture(true, fade_in_linear(1000), fade_out_linear(1000), move |state| {
            let state = state.clone();
            Some(MutableTransform::new(move |scope, _| {
                if state.pending_target_state().is_some() {
                    scope.set_offset(IntOffset::new(read.get(), 0));
                }
            }))
        });
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    state.defer(false);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    for step in steps {
        ui.advance_time_by_frame();
        preview.set(*step);
        ui.wait_for_idle();
    }
    let last = x.get();
    assert_eq!(last, *steps.last().unwrap() as f32);
    state.animate_to(false);
    for _ in 0..3 {
        ui.advance_time_by_frame();
    }
    x.get() - last
}

// Compose: compose/animation/animation/src/commonMain/kotlin/androidx/compose/animation/DeferredEnterExitTransition.kt, trackSlideVelocity (no test of Compose's without a provider)
#[test]
fn the_offset_s_velocity_is_tracked_without_a_provider_in_its_direction() {
    let right = overshoot_after(&[20, 40, 60, 80, 100]);
    let left = overshoot_after(&[-20, -40, -60, -80, -100]);
    assert!(right > 5.0, "goes on to the right: {right}");
    assert!(left < -5.0, "goes on to the left: {left}");
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/input/pointer/util/VelocityTracker.kt, HorizonMilliseconds (no test of Compose's through a hand-off)
#[test]
fn the_offset_s_velocity_counts_its_last_100_ms_only() {
    let slow: Vec<i32> = (1..=13).map(|step| 3000 + step * 10).collect();
    let after_fast: Vec<i32> = [1000, 2000, 3000].into_iter().chain(slow.iter().copied()).collect();
    let after_rest: Vec<i32> = [3000, 3000, 3000].into_iter().chain(slow.iter().copied()).collect();
    let (fast, rest) = (overshoot_after(&after_fast), overshoot_after(&after_rest));
    assert!(rest > 3.0, "it's moving: {rest}");
    assert_eq!(fast, rest, "what came 200 ms before doesn't count");
}

// Compose: compose/animation/animation/src/commonMain/kotlin/androidx/compose/animation/DeferredEnterExitTransition.kt, combinedSlide (no test of Compose's: manual changes add on during the catch-up)
#[test]
fn a_manual_change_during_the_catch_up_adds_on_top_of_it() {
    let at_third_frame = |changed: bool| {
        let preview = mutable_state_of(50);
        let read = preview.handle();
        let Fixture { mut ui, state, x, .. } = fixture(
            false,
            Some(slide_in_horizontally_with(linear(1000), |_| 200)),
            Some(slide_out_horizontally_with(linear(1000), |_| 200)),
            move |state| {
                let state = state.clone();
                Some(MutableTransform::new(move |scope, _| {
                    if state.pending_target_state().is_some() {
                        scope.set_offset(IntOffset::new(read.get(), 0));
                    }
                }))
            },
        );
        ui.wait_for_idle();
        ui.set_auto_advance(false);
        state.animate_to(true);
        ui.advance_time_by_frame();
        ui.advance_time_by(millis(500));
        state.defer(false);
        ui.advance_time_by_frame();
        ui.advance_time_by_frame();
        if changed {
            preview.set(80);
        }
        ui.advance_time_by_frame();
        x.get()
    };
    let (unchanged, changed) = (at_third_frame(false), at_third_frame(true));
    assert!(unchanged > 50.0, "still catching up from about 100: {unchanged}");
    close(changed - unchanged, 30.0, 1.0, "the change on top of the catch-up");
}

// Compose: compose/animation/animation-core/src/commonMain/kotlin/androidx/compose/animation/core/Transition.kt, updateTargetValue (isInterrupted = !isFinished && forcedInitialValue == null; no test of Compose's)
#[test]
fn a_hand_off_mid_animation_takes_the_exit_s_spec_not_the_interruption_spring() {
    let preview = mutable_state_of(0.5f32);
    let Fixture { mut ui, state, .. } = fixture(
        false,
        scale_in_linear(2000),
        scale_out_linear(1000),
        scale_while_pending(preview.handle()),
    );
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    state.animate_to(true);
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(200));
    state.defer(false);
    // Caught up, the enter still running under it.
    ui.advance_time_by(millis(320));
    ui.wait_for_idle();
    let full = ui.density().to_px(100.dp());
    close(width(&mut ui), full * 0.5, 1.0, "the preview's");
    state.animate_to(false);
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(500));
    // A second's tween from 0.5 to 0, half way: an interruption's spring would be at 0 by now.
    close(width(&mut ui), full * 0.25, full * 0.03, "along the exit's tween");
}

/// The operations drawn with a transform scaling and veiling a box, the veil over the parent or
/// not, in the deferred phase.
fn veil_ops(veil_match_parent_size: bool) -> Vec<DrawOp> {
    let Fixture { mut ui, state, .. } =
        fixture(true, fade_in_linear(100), fade_out_linear(100), move |_| {
            Some(
                MutableTransform::new(|scope, _| {
                    scope.set_scale(0.5);
                    scope.set_veil(Color::RED);
                })
                .veil_match_parent_size(veil_match_parent_size),
            )
        });
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    state.defer(false);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.host().display_list().ops.clone()
}

// Compose: compose/animation/animation/src/commonMain/kotlin/androidx/compose/animation/EnterExitTransition.kt, createEnterExitModifier (shouldVeilMatchParentSize: `activeMutableState.mutableData?.veilMatchParentSize`; no test of Compose's in the deferred phase)
#[test]
fn a_transform_s_veil_over_the_parent_is_not_scaled_and_one_not_is() {
    let position = |ops: &[DrawOp]| {
        let veil = ops
            .iter()
            .position(|op| matches!(op, DrawOp::DrawRect(_, paint) if paint.color == Color::RED));
        veil.unwrap_or_else(|| panic!("the veil: {ops:?}"))
    };
    let transformed = |ops: &[DrawOp]| ops.iter().any(|op| matches!(op, DrawOp::Transform { .. }));
    let over_parent = veil_ops(true);
    assert!(!transformed(&over_parent[..position(&over_parent)]), "{over_parent:?}");
    let over_content = veil_ops(false);
    assert!(transformed(&over_content[..position(&over_content)]), "{over_content:?}");
}

// Compose: compose/animation/animation/src/commonMain/kotlin/androidx/compose/animation/EnterExitTransition.kt, createGraphicsLayerBlock (forcedInitialValue = transformOriginHandoffValue; no test of Compose's)
#[test]
fn the_transform_origin_hands_off_from_the_manual_one() {
    let state = DeferredTransitionState::new(true);
    let held = state.clone();
    let mut ui = compose_test(move || {
        let transition = remember_deferred_transition(&held, None);
        let transform = remember(|| {
            Rc::new(MutableTransform::new(|scope, _| {
                scope.set_scale(0.5);
                scope.set_transform_origin(TransformOrigin {
                    pivot_fraction_x: 0.0,
                    pivot_fraction_y: 0.0,
                });
            }))
        });
        transition.DeferredAnimatedVisibility(
            |visible| *visible,
            Modifier,
            fade_in_linear(1000),
            fade_out_linear(1000),
            (*transform).clone(),
            |_| {
                Box!(modifier = Modifier.size(100.dp()).background(Color::BLUE));
            },
        );
    });
    ui.wait_for_idle();
    let pivot = |ui: &mut ComposeTestRule| {
        ui.host().display_list().ops.iter().find_map(|op| match op {
            DrawOp::Transform { pivot, .. } => Some(pivot.x),
            _ => None,
        })
    };
    ui.set_auto_advance(false);
    state.defer(false);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert_eq!(pivot(&mut ui), Some(0.0), "the manual origin");
    state.animate_to(false);
    ui.advance_time_by_frame();
    let centre = ui.density().to_px(50.dp());
    let handed_off = pivot(&mut ui).expect("a scale");
    assert!(handed_off < centre / 2.0, "from the manual origin: {handed_off}");
    ui.advance_time_by(millis(500));
    close(pivot(&mut ui).expect("a scale"), centre, 1.0, "at the centre, the exit's");
}
