//! `DeferredAnimatedContent` against Compose's `DeferredAnimatedContentTest`: a pending target's
//! content is composed but held, both contents can be moved by hand, and the transition takes up
//! from there; an interruption waits while it is pending. The contents are read from the tree:
//! `onGloballyPositioned` isn't called for a layer's change alone (Compose's is, D828).

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::DrawOp;
use compose_ui::{Alignment, Color, Dp, IntOffset, IntSize, Modifier, TransformOrigin, UnitExt};

fn millis(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

fn linear(duration: u32) -> TweenSpec {
    tween(duration).easing(LINEAR_EASING)
}

fn close(actual: f32, expected: f32, tolerance: f32, what: &str) {
    assert!((actual - expected).abs() <= tolerance, "{what}: {actual}, Compose's {expected}");
}

fn fades(duration: u32) -> ContentTransform {
    fade_in_with(linear(duration), 0.0).together_with(fade_out_with(linear(duration), 0.0))
}

fn scales(duration: u32) -> ContentTransform {
    scale_in_with(linear(duration), 0.0, TransformOrigin::CENTER).together_with(scale_out_with(
        linear(duration),
        0.0,
        TransformOrigin::CENTER,
    ))
}

type Scope = AnimatedContentTransitionScope<&'static str>;

/// A rule with a deferred content over `state`: a box of `size` tagged `content_` and its state.
fn rule_of(
    state: &DeferredTransitionState<&'static str>,
    spec: impl Fn(Scope) -> ContentTransform + 'static,
    mutable_spec: impl Fn(Scope) -> Option<MutableContentTransform> + 'static,
    size: impl Fn(&str) -> Dp + 'static,
) -> (ComposeTestRule, Rc<Cell<IntSize>>) {
    let held = state.clone();
    let container = Rc::new(Cell::new(IntSize::ZERO));
    let seen = container.clone();
    let (spec, mutable_spec, size) = (Rc::new(spec), Rc::new(mutable_spec), Rc::new(size));
    let ui = compose_test(move || {
        let transition = remember_deferred_transition(&held, None);
        let (spec, mutable_spec, size, seen) =
            (spec.clone(), mutable_spec.clone(), size.clone(), seen.clone());
        transition.DeferredAnimatedContent(
            Modifier.on_globally_positioned(move |coordinates: compose_ui::LayoutCoordinates| {
                seen.set(coordinates.size())
            }),
            move |scope| spec(scope),
            Alignment::TOP_START,
            ContentKey::state(),
            move |scope| mutable_spec(scope),
            move |_, target: &'static str| {
                Box!(modifier = Modifier.size(size(target)).test_tag(format!("content_{target}")));
            },
        );
    });
    (ui, container)
}

fn of_100(_: &str) -> Dp {
    100.dp()
}

fn no_transform(_: Scope) -> Option<MutableContentTransform> {
    None
}

/// A tagged box's left in the root in pixels.
fn left(ui: &mut ComposeTestRule, tag: &str) -> f32 {
    let bounds = ui.on_node_with_tag(tag).get_unclipped_bounds_in_root();
    ui.density().to_px(bounds.left)
}

fn top(ui: &mut ComposeTestRule, tag: &str) -> f32 {
    let bounds = ui.on_node_with_tag(tag).get_unclipped_bounds_in_root();
    ui.density().to_px(bounds.top)
}

/// A tagged box's width in the root in pixels, its layer's scale counted.
fn width(ui: &mut ComposeTestRule, tag: &str) -> f32 {
    let bounds = ui.on_node_with_tag(tag).get_bounds_in_root();
    ui.density().to_px(bounds.right - bounds.left)
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, waitToEnterTest
#[test]
fn a_pending_content_waits_composed_to_enter() {
    let state = DeferredTransitionState::new("A");
    let (mut ui, _) = rule_of(&state, |_| fades(100), no_transform, of_100);
    ui.on_node_with_tag("content_A").assert_is_displayed();
    ui.on_node_with_tag("content_B").assert_does_not_exist();
    state.defer("B");
    ui.wait_for_idle();
    ui.on_node_with_tag("content_A").assert_exists();
    ui.on_node_with_tag("content_B").assert_exists();
    state.animate_to("B");
    ui.wait_for_idle();
    ui.on_node_with_tag("content_A").assert_does_not_exist();
    ui.on_node_with_tag("content_B").assert_is_displayed();
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, immediateEnterTest
#[test]
fn a_content_animated_to_at_once_enters() {
    let state = DeferredTransitionState::new("A");
    let (mut ui, _) = rule_of(&state, |_| fades(100), no_transform, of_100);
    ui.on_node_with_tag("content_A").assert_is_displayed();
    state.animate_to("B");
    ui.wait_for_idle();
    ui.on_node_with_tag("content_B").assert_exists();
    ui.on_node_with_tag("content_A").assert_does_not_exist();
    ui.on_node_with_tag("content_B").assert_is_displayed();
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, immediateExitTest
// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, verifyExitAnimationAfterGateOpens
#[test]
fn the_old_content_stays_displayed_until_animated_to_and_its_exit_ends() {
    let state = DeferredTransitionState::new("A");
    let (mut ui, _) = rule_of(&state, |_| fades(1000), no_transform, of_100);
    ui.wait_for_idle();
    ui.on_node_with_tag("content_A").assert_is_displayed();
    state.defer("B");
    ui.set_auto_advance(false);
    ui.advance_time_by_frame();
    ui.on_node_with_tag("content_A").assert_is_displayed();
    ui.on_node_with_tag("content_B").assert_exists();
    state.animate_to("B");
    ui.advance_time_by_frame();
    ui.on_node_with_tag("content_A").assert_is_displayed();
    ui.advance_time_by(millis(500));
    ui.on_node_with_tag("content_A").assert_is_displayed();
    ui.advance_time_by(millis(500));
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    ui.on_node_with_tag("content_A").assert_does_not_exist();
    ui.on_node_with_tag("content_B").assert_is_displayed();
}

fn a_small_others_large(target: &str) -> Dp {
    if target == "A" { 100.dp() } else { 200.dp() }
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, sizeChangeDelayedUntilReady
#[test]
fn the_container_keeps_its_size_while_a_larger_content_is_pending() {
    let state = DeferredTransitionState::new("A");
    let (mut ui, size) = rule_of(&state, |_| fades(100), no_transform, a_small_others_large);
    let small = ui.density().round_to_px(100.dp());
    let large = ui.density().round_to_px(200.dp());
    ui.wait_for_idle();
    assert_eq!(size.get(), IntSize::new(small, small));
    state.defer("B");
    ui.wait_for_idle();
    assert_eq!(size.get(), IntSize::new(small, small));
    state.animate_to("B");
    ui.wait_for_idle();
    assert_eq!(size.get(), IntSize::new(large, large));
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, flappingTest
#[test]
fn an_animation_started_goes_on_through_a_defer_of_its_target() {
    let state = DeferredTransitionState::new("A");
    let spec = |_: Scope| fades(100).using(Some(size_transform(true, |_, _| linear(100).into())));
    let (mut ui, size) = rule_of(&state, spec, no_transform, a_small_others_large);
    ui.wait_for_idle();
    let small = ui.density().round_to_px(100.dp());
    let large = ui.density().round_to_px(200.dp());
    assert_eq!(size.get().width, small);
    state.animate_to("B");
    ui.set_auto_advance(false);
    ui.wait_for_idle();
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(50));
    assert!(size.get().width > small && size.get().width < large, "{:?}", size.get());
    let at_flap = size.get().width;
    state.defer("B");
    ui.wait_for_idle();
    ui.advance_time_by(millis(25));
    assert!(size.get().width > at_flap, "{:?} from {at_flap}", size.get());
    ui.advance_time_by(millis(100));
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_eq!(size.get().width, large);
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, verifyCurrentStateRemainsDuringPreparation
#[test]
fn the_transition_stays_at_its_state_while_prepared() {
    let state = DeferredTransitionState::new("A");
    let held: Rc<RefCell<Option<DeferredTransition<&'static str>>>> = Rc::default();
    let (remembered, shared) = (state.clone(), held.clone());
    let mut ui = compose_test(move || {
        let transition = remember_deferred_transition(&remembered, None);
        *shared.borrow_mut() = Some(transition.clone());
        transition.DeferredAnimatedContent(
            Modifier,
            |_| default_content_transform(),
            Alignment::TOP_START,
            ContentKey::state(),
            no_transform,
            |_, _| {
                Box!(modifier = Modifier.size(100.dp()));
            },
        );
    });
    let read = |ui: &mut ComposeTestRule| {
        ui.wait_for_idle();
        let held = held.borrow();
        let transition = held.as_ref().unwrap();
        (transition.current_state(), transition.target_state())
    };
    assert_eq!(read(&mut ui), ("A", "A"));
    state.defer("B");
    assert_eq!(read(&mut ui), ("A", "A"));
    assert_eq!(state.pending_target_state(), Some("B"));
    state.animate_to("B");
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(read(&mut ui).0, "B");
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, doubleChangeTargetWhileNotReady
#[test]
fn a_second_pending_target_keeps_the_first_s_content_until_settled() {
    let state = DeferredTransitionState::new("0");
    let (mut ui, _) = rule_of(&state, |_| fades(100), no_transform, of_100);
    ui.on_node_with_tag("content_0").assert_is_displayed();
    state.defer("1");
    ui.wait_for_idle();
    ui.on_node_with_tag("content_0").assert_is_displayed();
    ui.on_node_with_tag("content_1").assert_exists();
    state.defer("2");
    ui.wait_for_idle();
    ui.on_node_with_tag("content_0").assert_is_displayed();
    ui.on_node_with_tag("content_2").assert_exists();
    ui.on_node_with_tag("content_1").assert_exists();
    state.animate_to("2");
    ui.wait_for_idle();
    ui.on_node_with_tag("content_1").assert_does_not_exist();
}

fn b_large_others_small(target: &str) -> Dp {
    if target == "B" { 200.dp() } else { 100.dp() }
}

fn expanding(duration: u32) -> ContentTransform {
    (fade_in_with(linear(duration), 0.0)
        + expand_in_with(linear(duration), Alignment::BOTTOM_END, true, |_| IntSize::ZERO))
    .together_with(
        fade_out_with(linear(duration), 0.0)
            + shrink_out_with(linear(duration), Alignment::BOTTOM_END, true, |_| IntSize::ZERO),
    )
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, interruptionRespectedOnlyWhenReady
#[test]
fn an_interruption_starts_only_once_animated_to() {
    let state = DeferredTransitionState::new("A");
    let (mut ui, size) = rule_of(&state, |_| expanding(100), no_transform, b_large_others_small);
    ui.wait_for_idle();
    let small = ui.density().round_to_px(100.dp());
    state.animate_to("B");
    ui.set_auto_advance(false);
    ui.wait_for_idle();
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(50));
    assert!(size.get().width > small, "{:?}", size.get());
    state.defer("C");
    ui.wait_for_idle();
    ui.advance_time_by_frame();
    let after = size.get().width;
    ui.advance_time_by(millis(20));
    assert!(size.get().width > after, "goes on to B: {:?} from {after}", size.get());
    state.animate_to("C");
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    let before = size.get().width;
    ui.advance_time_by(millis(20));
    assert!(size.get().width < before, "shrinks to C: {:?} from {before}", size.get());
    ui.advance_time_by(millis(1000));
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    ui.on_node_with_tag("content_C").assert_is_displayed();
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, interruptionRespectedOnlyWhenReady_verifyComposition
#[test]
fn an_interruption_pending_keeps_all_three_contents_composed() {
    let state = DeferredTransitionState::new("A");
    let (mut ui, _) = rule_of(&state, |_| expanding(100), no_transform, b_large_others_small);
    ui.wait_for_idle();
    ui.on_node_with_tag("content_A").assert_is_displayed();
    state.animate_to("B");
    ui.set_auto_advance(false);
    ui.wait_for_idle();
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(50));
    ui.on_node_with_tag("content_A").assert_exists();
    ui.on_node_with_tag("content_B").assert_exists();
    ui.on_node_with_tag("content_C").assert_does_not_exist();
    state.defer("C");
    ui.wait_for_idle();
    ui.advance_time_by_frame();
    ui.on_node_with_tag("content_C").assert_exists();
    ui.on_node_with_tag("content_A").assert_exists();
    ui.on_node_with_tag("content_B").assert_exists();
    state.animate_to("C");
    ui.advance_time_by_frame();
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    ui.on_node_with_tag("content_C").assert_is_displayed();
    ui.on_node_with_tag("content_A").assert_does_not_exist();
    ui.on_node_with_tag("content_B").assert_does_not_exist();
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, predictiveBackMutableTransformDataTest
#[test]
fn the_transforms_are_given_their_contents_full_sizes() {
    let state = DeferredTransitionState::new("A");
    let sizes = Rc::new(Cell::new((IntSize::ZERO, IntSize::ZERO)));
    let (held, seen) = (state.clone(), sizes.clone());
    let transform = MutableContentTransform::new()
        .target_content_transform({
            let (held, seen) = (held.clone(), seen.clone());
            move |scope, size| {
                if held.pending_target_state().is_some_and(|pending| pending != "A") {
                    seen.set((size, seen.get().1));
                    scope.set_offset(IntOffset::ZERO);
                }
            }
        })
        .initial_content_transform(move |scope, size| {
            if held.pending_target_state().is_some_and(|pending| pending != "A") {
                seen.set((seen.get().0, size));
                scope.set_offset(IntOffset::ZERO);
            }
        });
    let (mut ui, _) = rule_of(
        &state,
        |_| default_content_transform(),
        move |_| Some(transform.clone()),
        a_small_others_large,
    );
    ui.on_node_with_tag("content_A").assert_is_displayed();
    state.defer("B");
    ui.wait_for_idle();
    let (enter, exit) = sizes.get();
    assert!(enter.width > 0, "{enter:?}");
    assert!(exit.width > 0, "{exit:?}");
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, animatedContent_previewTransforms_handoffCorrectly
#[test]
fn manual_offsets_hand_off_to_the_slides() {
    let state = DeferredTransitionState::new("A");
    let previewing = mutable_state_of(false);
    let preview = mutable_state_of(0);
    let (reading, offset) = (previewing.handle(), preview.handle());
    let (mut ui, _) = rule_of(
        &state,
        |_| {
            slide_in_horizontally_with(tween(160), |_| 200)
                .together_with(slide_out_horizontally_with(tween(160), |_| -200))
        },
        move |scope| {
            (reading.get() && *scope.target_state() != "A").then(|| {
                MutableContentTransform::new()
                    .target_content_transform(move |scope, _| {
                        scope.set_offset(IntOffset::new(offset.get(), 100))
                    })
                    .initial_content_transform(move |scope, _| {
                        scope.set_offset(IntOffset::new(-offset.get(), 100))
                    })
            })
        },
        of_100,
    );
    ui.wait_for_idle();
    assert_eq!(left(&mut ui, "content_A"), 0.0);
    ui.set_auto_advance(false);
    previewing.set(true);
    state.defer("B");
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    preview.set(50);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert_eq!(left(&mut ui, "content_B"), 50.0);
    assert_eq!(left(&mut ui, "content_A"), -50.0);
    assert_eq!(top(&mut ui, "content_B"), 100.0);
    assert_eq!(top(&mut ui, "content_A"), 100.0);
    previewing.set(false);
    state.animate_to("B");
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(80));
    ui.advance_time_by_frame();
    let (enter_x, exit_x) = (left(&mut ui, "content_B"), left(&mut ui, "content_A"));
    let (enter_y, exit_y) = (top(&mut ui, "content_B"), top(&mut ui, "content_A"));
    assert!(enter_x > 0.0 && enter_x < 50.0, "enter x: {enter_x}");
    assert!(exit_x < -50.0 && exit_x > -200.0, "exit x: {exit_x}");
    assert!(enter_y > 0.0 && enter_y < 100.0, "enter y: {enter_y}");
    assert!(exit_y > 0.0 && exit_y < 100.0, "exit y: {exit_y}");
    ui.advance_time_by(millis(1000));
    ui.advance_time_by_frame();
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_eq!(left(&mut ui, "content_B"), 0.0);
    ui.on_node_with_tag("content_A").assert_does_not_exist();
    ui.on_node_with_tag("content_B").assert_is_displayed();
}

/// What a mutable spec was last called with, and how many times.
#[derive(Clone, Default)]
struct Captured {
    states: Rc<RefCell<Option<(&'static str, &'static str)>>>,
    count: Rc<Cell<u32>>,
}

impl Captured {
    fn spec(&self) -> impl Fn(Scope) -> Option<MutableContentTransform> + 'static {
        let captured = self.clone();
        move |scope| {
            *captured.states.borrow_mut() = Some((*scope.initial_state(), *scope.target_state()));
            captured.count.set(captured.count.get() + 1);
            None
        }
    }

    fn states(&self) -> Option<(&'static str, &'static str)> {
        *self.states.borrow()
    }
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, previewScope_providesInitialAndTargetState
#[test]
fn the_mutable_spec_is_given_the_target_and_the_pending_state() {
    let state = DeferredTransitionState::new("A");
    let captured = Captured::default();
    let (mut ui, _) = rule_of(&state, |_| fades(100), captured.spec(), of_100);
    ui.wait_for_idle();
    state.defer("B");
    ui.wait_for_idle();
    assert_eq!(captured.states(), Some(("A", "B")));
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, previewScope_providesCorrectStates_onInterruption
#[test]
fn the_mutable_spec_is_called_for_a_deferred_phase_only() {
    let state = DeferredTransitionState::new("A");
    let captured = Captured::default();
    let (mut ui, _) = rule_of(&state, |_| fades(100), captured.spec(), of_100);
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    state.animate_to("B");
    ui.advance_time_by_frame();
    assert_eq!(captured.states(), None);
    state.defer("A");
    ui.advance_time_by_frame();
    assert_eq!(captured.states(), Some(("B", "A")));
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, previewScope_providesCorrectStates_onInterruptionDuringPreviewPhase
#[test]
fn deferring_the_target_itself_ends_the_preview_without_calling_the_mutable_spec() {
    let state = DeferredTransitionState::new("A");
    let captured = Captured::default();
    let (mut ui, _) = rule_of(&state, |_| fades(100), captured.spec(), of_100);
    ui.wait_for_idle();
    state.defer("B");
    ui.wait_for_idle();
    assert_eq!(captured.states(), Some(("A", "B")));
    let calls = captured.count.get();
    state.defer("A");
    ui.wait_for_idle();
    assert_eq!(captured.states(), Some(("A", "B")));
    assert_eq!(captured.count.get(), calls);
}

/// Both contents scaled by `preview` while the change isn't to `A`.
fn scale_both(
    preview: MutableState<f32>,
) -> impl Fn(Scope) -> Option<MutableContentTransform> + 'static {
    move |scope| {
        (*scope.target_state() != "A").then(|| {
            MutableContentTransform::new()
                .target_content_transform(move |scope, _| scope.set_scale(preview.get()))
                .initial_content_transform(move |scope, _| scope.set_scale(preview.get()))
        })
    }
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, animatedContent_previewTransforms_handoffVelocity
#[test]
fn a_faster_gesture_hands_off_greater_velocities_to_both_contents() {
    let state = DeferredTransitionState::new("A");
    let preview = mutable_state_of(1.0f32);
    let (mut ui, _) = rule_of(
        &state,
        |_| {
            let soft = || spring_with(Spring::DAMPING_RATIO_NO_BOUNCY, Spring::STIFFNESS_VERY_LOW);
            scale_in_with(soft(), 0.2, TransformOrigin::CENTER).together_with(scale_out_with(
                soft(),
                0.2,
                TransformOrigin::CENTER,
            ))
        },
        scale_both(preview.handle()),
        of_100,
    );
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    let gesture = |ui: &mut ComposeTestRule, steps: &[f32]| {
        state.defer("B");
        ui.advance_time_by_frame();
        ui.wait_for_idle();
        for step in steps {
            ui.advance_time_by_frame();
            preview.set(*step);
            ui.wait_for_idle();
        }
        let (enter, exit) = (width(ui, "content_B"), width(ui, "content_A"));
        state.animate_to("B");
        for _ in 0..3 {
            ui.advance_time_by_frame();
        }
        ui.wait_for_idle();
        (enter - width(ui, "content_B"), exit - width(ui, "content_A"))
    };
    let slow = gesture(&mut ui, &[0.9, 0.8, 0.7, 0.6, 0.5]);
    ui.set_auto_advance(true);
    preview.set(1.0);
    state.animate_to("A");
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    let fast = gesture(&mut ui, &[0.9, 0.7, 0.5]);
    assert!(fast.0 > slow.0, "enter: {} over {}", fast.0, slow.0);
    assert!(fast.1 > slow.1, "exit: {} over {}", fast.1, slow.1);
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, animatedContent_previewScale_interrupt_deferred_by_original_state_is_seamless
#[test]
fn animating_to_the_original_state_goes_back_from_the_manual_scale() {
    let state = DeferredTransitionState::new("A");
    let preview = mutable_state_of(1.0f32);
    let (mut ui, _) = rule_of(&state, |_| scales(1000), scale_both(preview.handle()), of_100);
    ui.wait_for_idle();
    let full = width(&mut ui, "content_A");
    ui.set_auto_advance(false);
    state.defer("B");
    preview.set(0.8);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    close(width(&mut ui, "content_A"), full * 0.8, 1.0, "the preview's");
    state.animate_to("A");
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    let after = width(&mut ui, "content_A");
    close(after, full * 0.8, 1.0, "no jump");
    ui.advance_time_by(millis(100));
    ui.wait_for_idle();
    let later = width(&mut ui, "content_A");
    assert!(later > after, "growing: {later} from {after}");
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    close(width(&mut ui, "content_A"), full, 1.0, "at rest");
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, animatedContent_previewScaleTransforms_handoffUsesTweenSpec
#[test]
fn a_hand_off_takes_the_enter_s_tween_from_the_manual_scale() {
    let state = DeferredTransitionState::new("A");
    let previewing = mutable_state_of(false);
    let preview = mutable_state_of(1.0f32);
    let (reading, scale) = (previewing.handle(), preview.handle());
    let (mut ui, _) = rule_of(
        &state,
        |_| scales(1000),
        move |scope| reading.get().then(|| scale_both(scale)(scope)).flatten(),
        of_100,
    );
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    previewing.set(true);
    state.defer("B");
    preview.set(0.5);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    previewing.set(false);
    state.animate_to("B");
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    ui.advance_time_by(millis(500));
    ui.wait_for_idle();
    let full = ui.density().to_px(100.dp());
    close(width(&mut ui, "content_B"), full * 0.75, 2.0, "half way from 0.5 to 1");
}

/// Exits by the change, what each state's exit was last computed as.
#[derive(Clone, Default)]
struct ExitsOf {
    a: Rc<RefCell<Option<ExitTransition>>>,
    b: Rc<RefCell<Option<ExitTransition>>>,
}

impl ExitsOf {
    fn spec(
        &self,
        exit_millis: impl Fn(&str, &str) -> u32 + 'static,
    ) -> impl Fn(Scope) -> ContentTransform + 'static {
        let exits = self.clone();
        move |scope| {
            let (initial, target) = (*scope.initial_state(), *scope.target_state());
            let spec = fade_in().together_with(match exit_millis(initial, target) {
                0 => fade_out(),
                millis => fade_out_with(tween(millis), 0.0),
            });
            if initial == "A" {
                *exits.a.borrow_mut() = Some(spec.initial_content_exit.clone());
            }
            if initial == "B" {
                *exits.b.borrow_mut() = Some(spec.initial_content_exit.clone());
            }
            spec
        }
    }
}

fn faded_out(millis: u32) -> Option<ExitTransition> {
    Some(fade_out_with(tween(millis), 0.0))
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, animatedContent_interruption_during_deferred_phase_uses_correct_spec
#[test]
fn an_interruption_of_a_deferred_phase_takes_the_new_change_s_exits() {
    let state = DeferredTransitionState::new("A");
    let exits = ExitsOf::default();
    let spec = exits.spec(|initial, target| match (initial, target) {
        ("A", "B") => 100,
        ("A", "C") => 500,
        ("B", "A") => 800,
        _ => 0,
    });
    let (mut ui, _) = rule_of(&state, spec, no_transform, of_100);
    ui.wait_for_idle();
    state.defer("B");
    ui.wait_for_idle();
    assert_eq!(*exits.a.borrow(), faded_out(100));
    state.animate_to("C");
    ui.wait_for_idle();
    assert_eq!(*exits.a.borrow(), faded_out(500), "A's for A to C");
    assert_eq!(*exits.b.borrow(), faded_out(800), "B's for B to A");
    ui.on_node_with_tag("content_C").assert_is_displayed();
    ui.on_node_with_tag("content_A").assert_does_not_exist();
    ui.on_node_with_tag("content_B").assert_does_not_exist();
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, animatedContent_interruption_during_regular_phase_uses_correct_spec
#[test]
fn an_interruption_of_an_animation_leaves_the_exit_already_running() {
    let state = DeferredTransitionState::new("A");
    let exits = ExitsOf::default();
    let spec = exits.spec(|initial, target| match (initial, target) {
        ("A", "B") => 100,
        ("A", "C") => 500,
        ("B", "C") => 800,
        _ => 0,
    });
    let (mut ui, _) = rule_of(&state, spec, no_transform, of_100);
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    state.animate_to("B");
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(*exits.a.borrow(), faded_out(100));
    state.animate_to("C");
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    ui.on_node_with_tag("content_C").assert_exists();
    ui.on_node_with_tag("content_A").assert_exists();
    ui.on_node_with_tag("content_B").assert_exists();
    assert_eq!(*exits.a.borrow(), faded_out(100), "A's goes on");
    assert_eq!(*exits.b.borrow(), faded_out(800), "B's for B to C");
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    ui.on_node_with_tag("content_C").assert_is_displayed();
    ui.on_node_with_tag("content_A").assert_does_not_exist();
    ui.on_node_with_tag("content_B").assert_does_not_exist();
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, nonMutatingNodeDoesNotReceiveStaleMutations
#[test]
fn a_content_not_moved_by_a_new_phase_has_no_stale_manual_offset() {
    let state = DeferredTransitionState::new("A");
    let (mut ui, _) = rule_of(
        &state,
        |_| {
            (fade_in_with(linear(1000), 0.0) + slide_in_with(linear(1000), |_| IntOffset::ZERO))
                .together_with(
                    fade_out_with(linear(1000), 0.0)
                        + slide_out_with(linear(1000), |_| IntOffset::ZERO),
                )
        },
        |scope| {
            (*scope.initial_state() == "A" && *scope.target_state() == "B").then(|| {
                MutableContentTransform::new().initial_content_transform(|scope, _| {
                    scope.set_offset(IntOffset::new(100, 100))
                })
            })
        },
        of_100,
    );
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    state.defer("B");
    ui.advance_time_by_frame();
    assert_eq!(left(&mut ui, "content_A"), 100.0);
    state.animate_to("B");
    ui.advance_time_by(millis(100));
    ui.wait_for_idle();
    let before = left(&mut ui, "content_A");
    assert!((80.0..=95.0).contains(&before), "going back to 0: {before}");
    state.defer("C");
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    let after = left(&mut ui, "content_A");
    assert!(after < 100.0, "no jump back: {after} after {before}");
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, deferAfterExitFinishedRecoversExitStateTest
#[test]
fn deferring_a_content_whose_exit_ended_does_not_bring_it_back_at_once() {
    let state = DeferredTransitionState::new("A");
    let last = Rc::new(Cell::new(0.0f32));
    let (held, seen) = (state.clone(), last.clone());
    let mut ui = compose_test(move || {
        let transition = remember_deferred_transition(&held, None);
        let seen = seen.clone();
        transition.DeferredAnimatedContent(
            Modifier,
            |_| {
                slide_in_horizontally_with(linear(100_000), |_| 100)
                    .together_with(slide_out_horizontally_with(linear(100), |_| -100))
            },
            Alignment::TOP_START,
            ContentKey::state(),
            |_| {
                Some(
                    MutableContentTransform::new()
                        .initial_content_transform(|_, _| {})
                        .target_content_transform(|_, _| {}),
                )
            },
            move |_, target: &'static str| {
                let seen = seen.clone();
                Box!(
                    modifier = Modifier.size(100.dp()).on_globally_positioned(
                        move |coordinates: compose_ui::LayoutCoordinates| {
                            if target == "A" {
                                seen.set(coordinates.position_in_root().x);
                            }
                        }
                    )
                );
            },
        );
    });
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    state.animate_to("B");
    ui.advance_time_by(millis(5000));
    ui.wait_for_idle();
    assert!(last.get() < -90.0, "off-screen: {}", last.get());
    state.defer("A");
    ui.advance_time_by(millis(1));
    ui.wait_for_idle();
    assert!(last.get() < -90.0, "stays off-screen: {}", last.get());
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, testVeilMatchParentSize_isPreservedDuringHandoff
#[test]
fn a_veil_over_the_parent_stays_so_through_the_hand_off() {
    let state = DeferredTransitionState::new("A");
    let (mut ui, _) = rule_of(
        &state,
        |_| fade_in_with(tween(1000), 0.0).together_with(fade_out_with(tween(1000), 0.0)),
        |_| {
            Some(MutableContentTransform::of(
                MutableTransform::empty(),
                MutableTransform::new(|scope, _| {
                    scope.set_scale(0.5);
                    scope.set_veil(Color::RED);
                })
                .veil_match_parent_size(true),
            ))
        },
        |_| 200.dp(),
    );
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    state.defer("B");
    ui.advance_time_by(millis(80));
    ui.wait_for_idle();
    state.animate_to("B");
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    // Compose's test reads a corner pixel: the veil, unlike the content, isn't scaled, so it
    // reaches the corner (D814: over the content's bounds, here the parent's).
    let full = ui.density().to_px(200.dp());
    let ops = &ui.host().display_list().ops;
    let veil = ops.iter().position(|op| {
        matches!(op, DrawOp::DrawRect(rect, paint)
            if paint.color.red() > paint.color.green() && paint.color.alpha() > 0
                && rect.left == 0.0 && rect.right == full)
    });
    let veil = veil.unwrap_or_else(|| panic!("the veil, at the content's full size: {ops:?}"));
    assert!(
        !ops[..veil].iter().any(|op| matches!(op, DrawOp::Transform { .. })),
        "the veil isn't transformed: {ops:?}"
    );
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, animatedContent_manualGesture_snapsWhenIdle_springsWhenAnimating
#[test]
fn a_manual_scale_applies_at_once_at_rest_and_springs_mid_animation() {
    let state = DeferredTransitionState::new("A");
    let preview = mutable_state_of(1.0f32);
    let scale = preview.handle();
    let (mut ui, _) = rule_of(
        &state,
        |_| scales(1000),
        move |_| {
            Some(
                MutableContentTransform::new()
                    .target_content_transform(move |scope, _| scope.set_scale(scale.get()))
                    .initial_content_transform(move |scope, _| scope.set_scale(scale.get())),
            )
        },
        of_100,
    );
    ui.wait_for_idle();
    let full = width(&mut ui, "content_A");
    ui.set_auto_advance(false);
    state.defer("B");
    preview.set(0.5);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    close(width(&mut ui, "content_A"), full * 0.5, 1.0, "at once at rest");
    ui.set_auto_advance(true);
    preview.set(1.0);
    state.animate_to("A");
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    state.animate_to("B");
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(100));
    state.defer("A");
    preview.set(0.5);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    let catching_up = width(&mut ui, "content_A");
    assert!((catching_up - full * 0.5).abs() > 5.0, "no snap: {catching_up} of {full}");
    // A's exit goes on under the catch-up, and takes it away when it ends: the last seen is what
    // Compose's test reads, caught up by then.
    let mut last = catching_up;
    while !ui.on_all_nodes(has_test_tag("content_A")).fetch_semantics_nodes().is_empty() {
        last = width(&mut ui, "content_A");
        ui.advance_time_by_frame();
    }
    close(last, full * 0.5, 1.0, "caught up");
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, testUnmutatedPropertiesDoNotJump
#[test]
fn what_a_transform_does_not_set_does_not_jump() {
    let state = DeferredTransitionState::new("A");
    let (mut ui, _) =
        rule_of(
            &state,
            |_| scales(1000),
            |_| {
                Some(MutableContentTransform::new().target_content_transform(|scope, _| {
                    scope.set_offset(IntOffset::new(100, 100))
                }))
            },
            of_100,
        );
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    state.animate_to("B");
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(500));
    ui.wait_for_idle();
    let halfway = width(&mut ui, "content_B");
    assert!(halfway > 10.0, "half way: {halfway}");
    state.defer("A");
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    ui.advance_time_by(millis(100));
    ui.wait_for_idle();
    let after = width(&mut ui, "content_B");
    assert!(after < halfway + 50.0, "no jump: {after} after {halfway}");
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, animatedContent_consecutiveGestures_resetsTransformScope
#[test]
fn a_new_gesture_does_not_take_the_last_one_s_offset() {
    let state = DeferredTransitionState::new("C");
    let gesture = mutable_state_of(IntOffset::ZERO);
    let offset = gesture.handle();
    let last = Rc::new(Cell::new(0.0f32));
    let (held, seen) = (state.clone(), last.clone());
    let mut ui = compose_test(move || {
        let transition = remember_deferred_transition(&held, None);
        let seen = seen.clone();
        transition.DeferredAnimatedContent(
            Modifier,
            |_| {
                slide_in_horizontally_with(linear(1000), |width| -width)
                    .together_with(slide_out_horizontally_with(linear(1000), |width| -width))
            },
            Alignment::TOP_START,
            ContentKey::state(),
            move |scope: AnimatedContentTransitionScope<&'static str>| {
                Some(if *scope.target_state() == "B" {
                    MutableContentTransform::new()
                        .target_content_transform(move |scope, _| scope.set_offset(offset.get()))
                } else {
                    MutableContentTransform::new()
                })
            },
            move |_, target: &'static str| {
                let seen = seen.clone();
                Box!(
                    modifier = Modifier.size(100.dp()).on_globally_positioned(
                        move |coordinates: compose_ui::LayoutCoordinates| {
                            if target == "B" {
                                seen.set(coordinates.position_in_root().x);
                            }
                        }
                    )
                );
            },
        );
    });
    ui.wait_for_idle();
    gesture.set(IntOffset::new(-80, 0));
    state.defer("B");
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    state.animate_to("B");
    ui.advance_time_by_frame();
    state.defer("A");
    ui.advance_time_by_frame();
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert!(last.get() > -20.0, "not pulled back to -80: {}", last.get());
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, deferredTransition_evaluatesTransitionSpec_withPendingScope
#[test]
fn the_transition_spec_is_evaluated_for_the_pending_change() {
    let state = DeferredTransitionState::new("A");
    let evaluated: Rc<RefCell<Vec<(&str, &str)>>> = Rc::default();
    let seen = evaluated.clone();
    let (mut ui, _) = rule_of(
        &state,
        move |scope| {
            seen.borrow_mut().push((*scope.initial_state(), *scope.target_state()));
            fades(100)
        },
        no_transform,
        of_100,
    );
    ui.wait_for_idle();
    state.animate_to("B");
    ui.wait_for_idle();
    evaluated.borrow_mut().clear();
    state.defer("A");
    ui.wait_for_idle();
    let evaluated = evaluated.borrow();
    assert!(evaluated.contains(&("B", "A")), "{evaluated:?}");
    assert!(!evaluated.contains(&("A", "B")), "{evaluated:?}");
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, animatedContent_identicalContentKey_gestureAndTransition_isStaticWithoutOffset
#[test]
fn states_of_one_key_stay_still_through_a_gesture_and_its_transition() {
    #[derive(Clone, Copy, PartialEq, Debug)]
    struct Page(&'static str, i32);
    let state = DeferredTransitionState::new(Page("C", 1));
    let offset = Rc::new(Cell::new(Offset2::default()));
    let (held, seen) = (state.clone(), offset.clone());
    let mut ui = compose_test(move || {
        let transition = remember_deferred_transition(&held, None);
        let transform = remember(|| {
            let moved = IntOffset::new(-100, -100);
            Rc::new(
                MutableContentTransform::new()
                    .target_content_transform(move |scope, _| scope.set_offset(moved))
                    .initial_content_transform(move |scope, _| scope.set_offset(moved)),
            )
        });
        let seen = seen.clone();
        transition.DeferredAnimatedContent(
            Modifier,
            |_| {
                (slide_in_horizontally_with(tween(300), |_| 500)
                    + slide_in_vertically_with(tween(300), |_| 500))
                .together_with(
                    slide_out_horizontally_with(tween(300), |_| -500)
                        + slide_out_vertically_with(tween(300), |_| -500),
                )
            },
            Alignment::TOP_START,
            ContentKey::of(|page: &Page| page.0),
            move |_| Some((*transform).clone()),
            move |_, _| {
                let seen = seen.clone();
                Box!(
                    modifier = Modifier.size(100.dp()).on_globally_positioned(
                        move |coordinates: compose_ui::LayoutCoordinates| {
                            let at = coordinates.position_in_root();
                            seen.set(Offset2 { x: at.x, y: at.y });
                        }
                    )
                );
            },
        );
    });
    let at_origin = |offset: &Rc<Cell<Offset2>>| {
        assert!(offset.get().x.abs() <= 0.1 && offset.get().y.abs() <= 0.1, "{:?}", offset.get());
    };
    ui.wait_for_idle();
    at_origin(&offset);
    state.defer(Page("C", 2));
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    at_origin(&offset);
    state.animate_to(Page("C", 2));
    ui.wait_for_idle();
    at_origin(&offset);
}

#[derive(Clone, Copy, Default, Debug)]
struct Offset2 {
    x: f32,
    y: f32,
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/DeferredAnimatedContentTest.kt, mutableContentTransform_fromTwoMutableTransforms_appliesInitialAndTargetTransformsDuringDefer
#[test]
fn two_transforms_given_apply_to_their_contents_during_a_defer() {
    let state = DeferredTransitionState::new("0");
    let initial = MutableTransform::new(|scope, _| scope.set_offset(IntOffset::new(-80, 0)));
    let target = MutableTransform::new(|scope, _| scope.set_offset(IntOffset::new(120, 0)));
    let (mut ui, _) = rule_of(
        &state,
        |_| EnterTransition::NONE.together_with(ExitTransition::NONE),
        move |_| Some(MutableContentTransform::of(initial.clone(), target.clone())),
        of_100,
    );
    ui.wait_for_idle();
    assert_eq!(left(&mut ui, "content_0"), 0.0);
    state.defer("1");
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(left(&mut ui, "content_0"), -80.0);
    assert_eq!(left(&mut ui, "content_1"), 120.0);
}
