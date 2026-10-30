//! `Modifier.anchored_draggable`, its state and its anchors: scenarios of Compose's
//! `AnchoredDraggableGestureTest`, `AnchoredDraggableStateTest`, `AnchoredDraggableDecayAnimationTest`
//! and `DraggableAnchorsTest`, rewritten.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_animation::spring_with;
use compose_foundation::gestures::{
    AnchoredDraggableDefaults, AnchoredDraggableModifier, AnchoredDraggableOptions,
    AnchoredDraggableState, DraggableAnchors, ScrollScope, anchored_fling_behavior,
    remember_anchored_fling_behavior,
};
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_test::*;
use compose_ui::{IntSize, LOCAL_LAYOUT_DIRECTION, LayoutDirection, Offset, UnitExt};

use crate::scroll_support::{launch, run};

#[derive(Clone, Copy, PartialEq, Debug)]
enum Value {
    A,
    B,
    C,
    D,
}

use Value::{A, B, C, D};

fn anchors(positions: &[(Value, f32)]) -> DraggableAnchors<Value> {
    DraggableAnchors::new(|config| {
        for (anchor, position) in positions {
            config.at(*anchor, *position);
        }
    })
}

fn abc() -> DraggableAnchors<Value> {
    anchors(&[(A, 0.0), (B, 100.0), (C, 200.0)])
}

fn rule() -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(300, 300), || {})
}

/// A 200 x 200 box dragging `state` along `orientation`, moved by it.
fn dragged(
    state: AnchoredDraggableState<Value>,
    orientation: Orientation,
    options: AnchoredDraggableOptions,
) -> ComposeTestRule {
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
        Box!(
            modifier = Modifier.size(200.dp()).anchored_draggable(
                state.clone(),
                orientation,
                options.clone()
            )
        );
    });
    ui.wait_for_idle();
    ui
}

fn no_slop(ui: &mut ComposeTestRule) {
    ui.host().set_view_configuration(Some(compose_ui::ViewConfiguration {
        touch_slop: 0.0,
        ..compose_ui::ViewConfiguration::default()
    }));
    ui.advance_time_by_frame();
}

fn swipe(ui: &mut ComposeTestRule, from: Offset, to: Offset) {
    ui.on_root().perform_touch_input(|touch| touch.swipe(from, to, 200));
    ui.wait_for_idle();
}

// Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/gestures/DraggableAnchorsTest.kt, draggableAnchors_builder_containsAll, draggableAnchors_get_nonexistentAnchor_returnsNaN
#[test]
fn anchors_hold_what_was_given() {
    let anchors = abc();
    assert_eq!(anchors.size(), 3);
    assert_eq!(anchors.position_of(&A), 0.0);
    assert_eq!(anchors.position_of(&C), 200.0);
    assert!(anchors.position_of(&D).is_nan());
    assert_eq!(anchors.anchor_at(1), Some(B));
    assert_eq!(anchors.position_at(1), 100.0);
    assert!(anchors.position_at(9).is_nan());
    let mut seen = Vec::new();
    anchors.for_each(|key, position| seen.push((*key, position)));
    assert_eq!(seen, [(A, 0.0), (B, 100.0), (C, 200.0)]);
}

// Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/gestures/DraggableAnchorsTest.kt, draggableAnchors_findsClosestAnchor, draggableAnchors_findsClosestAnchor_directional, draggableAnchors_closestAnchor_returnsNullWhenEmpty
#[test]
fn the_closest_anchor_is_found_any_way_or_one_way() {
    let anchors = abc();
    assert_eq!(anchors.closest_anchor(40.0), Some(A));
    assert_eq!(anchors.closest_anchor(60.0), Some(B));
    assert_eq!(anchors.closest_anchor(50.0), Some(B), "between two, the later");
    assert_eq!(anchors.closest_anchor(250.0), Some(C));
    assert_eq!(anchors.closest_anchor_towards(40.0, true), Some(B));
    assert_eq!(anchors.closest_anchor_towards(160.0, false), Some(B));
    // Nothing that way: Compose's search ends on the last anchor it looked at.
    assert_eq!(anchors.closest_anchor_towards(250.0, true), Some(C));
    let empty = DraggableAnchors::<Value>::new(|_| {});
    assert_eq!(empty.closest_anchor(0.0), None);
    assert_eq!(empty.closest_anchor_towards(0.0, true), None);
}

// Compose: compose/foundation/foundation/src/androidHostTest/kotlin/androidx/compose/foundation/gestures/DraggableAnchorsTest.kt, draggableAnchors_minPosition, draggableAnchors_maxPosition, draggableAnchors_hasPositionFor, draggableAnchors_equality_equalAnchors, draggableAnchors_equality_inequalAnchors
#[test]
fn anchors_know_their_bounds_and_compare_by_value() {
    let scattered = anchors(&[(A, 40.0), (B, -100.0), (C, 300.0), (D, 50.0)]);
    assert_eq!(scattered.min_position(), -100.0);
    assert_eq!(scattered.max_position(), 300.0);
    assert!(scattered.has_position_for(&C));
    assert!(!abc().has_position_for(&D));
    assert!(DraggableAnchors::<Value>::new(|_| {}).min_position().is_nan());
    assert_eq!(abc(), abc());
    assert_ne!(abc(), anchors(&[(A, 0.0), (B, 100.0), (C, 201.0)]));
    assert_ne!(abc(), anchors(&[(A, 0.0), (B, 100.0)]));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_constructorWithAnchors_updatesAnchorsAndInitializes, anchoredDraggable_constructorWithAnchors_initialValueNotInAnchors_updatesCurrentValue
#[test]
fn a_state_with_anchors_starts_at_its_value() {
    let state = AnchoredDraggableState::with_anchors(B, abc());
    assert_eq!(state.current_value(), B);
    assert_eq!(state.require_offset(), 100.0);
    assert_eq!(state.anchors(), abc());
    let outside = AnchoredDraggableState::with_anchors(D, abc());
    assert_eq!(outside.current_value(), D);
    assert!(outside.offset().is_nan());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_requireOffset_accessedInInitialComposition_throws
#[test]
#[should_panic(expected = "offset was read before being initialized")]
fn the_offset_before_the_anchors_is_refused() {
    AnchoredDraggableState::new(A).require_offset();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_targetValue_shouldBeEqualToCurrentValue_multipleAnchorsAtSameOffset
#[test]
fn anchors_at_one_place_stay_the_value_they_are() {
    let state = AnchoredDraggableState::with_anchors(
        B,
        anchors(&[(A, 1000.0), (B, 0.0), (C, 0.0), (D, 0.0)]),
    );
    assert_eq!((state.current_value(), state.target_value()), (B, B));
    let mut ui = rule();
    let snapping = state.clone();
    run(&mut ui, async move {
        snapping.snap_to(C).await.expect("nothing else");
    });
    assert_eq!((state.current_value(), state.target_value()), (C, C));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_targetState_updatedWithDeltaDispatch, anchoredDraggable_currentValue_updatedWithDeltaDispatch
#[test]
fn a_raw_delta_moves_it_and_its_values() {
    let state = AnchoredDraggableState::with_anchors(A, abc());
    assert_eq!(state.dispatch_raw_delta(60.0), 60.0);
    assert_eq!(state.require_offset(), 60.0);
    assert_eq!(state.target_value(), B);
    assert_eq!(state.current_value(), B, "past half way");
    // Clamped at the anchors.
    assert_eq!(state.dispatch_raw_delta(1000.0), 140.0);
    assert_eq!(state.require_offset(), 200.0);
    assert_eq!(state.current_value(), C);
    assert_eq!(state.settled_value(), A, "no drag ended");
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_progress
#[test]
fn progress_goes_from_0_to_1_between_two_anchors_either_way() {
    let state = AnchoredDraggableState::with_anchors(A, abc());
    assert_eq!(state.progress(&A, &B), 0.0);
    state.dispatch_raw_delta(25.0);
    assert_eq!(state.progress(&A, &B), 0.25);
    assert_eq!(state.progress(&B, &A), 0.75);
    state.dispatch_raw_delta(75.0);
    assert_eq!(state.progress(&A, &B), 1.0);
    assert_eq!(state.progress(&B, &C), 0.0);
    assert_eq!(state.progress(&A, &A), 1.0, "no distance");
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_snapTo_updatesImmediately
#[test]
fn a_snap_is_at_once() {
    let state = AnchoredDraggableState::with_anchors(A, abc());
    let mut ui = rule();
    let snapping = state.clone();
    run(&mut ui, async move {
        snapping.snap_to(C).await.expect("nothing else");
    });
    assert_eq!(state.current_value(), C);
    assert_eq!(state.settled_value(), C);
    assert_eq!(state.require_offset(), 200.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_targetState_updatedWithAnimation
#[test]
fn an_animation_goes_there_its_target_said_at_once() {
    let state = AnchoredDraggableState::with_anchors(A, abc());
    let mut ui = rule();
    ui.set_auto_advance(false);
    let animating = state.clone();
    launch(async move {
        let _ = animating.animate_to(C).await;
    });
    ui.advance_time_by_frame();
    assert_eq!(state.target_value(), C);
    assert!(state.is_animation_running());
    assert!(state.require_offset() < 200.0);
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_eq!(state.require_offset(), 200.0);
    assert_eq!(state.current_value(), C);
    assert!(!state.is_animation_running());
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_animateTo_animatesBeyondBounds
#[test]
fn an_animation_may_overshoot_the_anchors() {
    let state = AnchoredDraggableState::with_anchors(A, anchors(&[(A, 0.0), (C, 500.0)]));
    let mut ui = rule();
    ui.set_auto_advance(false);
    let animating = state.clone();
    let spec = spring_with(0.2, 1500.0);
    launch(async move {
        let _ = animating.animate_to_with(C, spec).await;
    });
    let mut highest = 0.0f32;
    for _ in 0..120 {
        ui.advance_time_by_frame();
        if !state.offset().is_nan() {
            highest = highest.max(state.offset());
        }
    }
    assert!(highest > 500.0, "{highest}");
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_targetNotInAnchors_emptyAnchors_animateTo_updatesCurrentValue, anchoredDraggable_targetNotInAnchors_animateTo_updatesCurrentValue, anchoredDraggable_targetNotInAnchors_emptyAnchors_snapTo_updatesCurrentValue, anchoredDraggable_targetNotInAnchors_snapTo_updatesCurrentValue
#[test]
fn a_target_not_among_the_anchors_is_the_value_at_once() {
    for with_anchors in [false, true] {
        for animate in [false, true] {
            let state = if with_anchors {
                AnchoredDraggableState::with_anchors(A, anchors(&[(A, 0.0), (B, 100.0)]))
            } else {
                AnchoredDraggableState::new(A)
            };
            let mut ui = rule();
            let going = state.clone();
            run(&mut ui, async move {
                if animate {
                    going.animate_to(C).await.expect("nothing else");
                } else {
                    going.snap_to(C).await.expect("nothing else");
                }
            });
            assert_eq!(state.current_value(), C);
            assert_eq!(state.settled_value(), C);
            if with_anchors {
                assert_eq!(state.require_offset(), 0.0, "nowhere to go");
            }
        }
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_updateAnchors_noOngoingDrag_shouldUpdateOffset
#[test]
fn new_anchors_with_no_drag_move_it_at_once() {
    let state = AnchoredDraggableState::new(A);
    assert!(state.offset().is_nan());
    state.update_anchors(abc());
    assert_eq!(state.require_offset(), 0.0);
    state.update_anchors(anchors(&[(A, 50.0), (B, 100.0)]));
    assert_eq!(state.current_value(), A);
    assert_eq!(state.require_offset(), 50.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_updateAnchors_ongoingDrag_shouldRestartDrag, anchoredDraggable_updateAnchors_anchoredDrag_invokedWithLatestAnchors
#[test]
fn new_anchors_restart_a_drag_with_them() {
    let state = AnchoredDraggableState::new(1);
    let seen: Rc<RefCell<Vec<usize>>> = Rc::default();
    let (dragging, noted) = (state.clone(), seen.clone());
    let mut ui = rule();
    let job = launch(async move {
        let _ = dragging
            .anchored_drag(MutatePriority::Default, |_, anchors| {
                noted.borrow_mut().push(anchors.size());
                std::future::pending::<()>()
            })
            .await;
    });
    ui.wait_for_idle();
    assert_eq!(*seen.borrow(), [0]);
    state.update_anchors(DraggableAnchors::new(|config| {
        config.at(1, 100.0);
        config.at(2, 200.0);
    }));
    ui.wait_for_idle();
    assert_eq!(*seen.borrow(), [0, 2]);
    assert!(job.is_active());
    job.cancel();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_updateAnchors_anchoredDrag_invokedWithLatestTarget
#[test]
fn new_anchors_and_target_restart_a_drag_with_them() {
    let state = AnchoredDraggableState::new(A);
    state.update_anchors(anchors(&[(A, 0.0), (B, 200.0)]));
    let seen: Rc<RefCell<Vec<Value>>> = Rc::default();
    let (dragging, noted) = (state.clone(), seen.clone());
    let mut ui = rule();
    let job = launch(async move {
        let _ = dragging
            .anchored_drag_to(B, MutatePriority::Default, |_, _, target| {
                noted.borrow_mut().push(target);
                std::future::pending::<()>()
            })
            .await;
    });
    ui.wait_for_idle();
    assert_eq!(*seen.borrow(), [B]);
    state.update_anchors_to(anchors(&[(A, 100.0), (B, 200.0)]), A);
    ui.wait_for_idle();
    assert_eq!(*seen.borrow(), [B, A]);
    job.cancel();
    ui.wait_for_idle();
    assert!(!state.is_animation_running(), "the target goes with the drag");
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_customDrag_doesNotSnapToClosestAnchor
#[test]
fn a_custom_drag_stays_where_it_went() {
    let state =
        AnchoredDraggableState::with_anchors(A, anchors(&[(A, 0.0), (B, 200.0), (C, 300.0)]));
    let mut ui = rule();
    for (to, value) in [(150.0, B), (260.0, C)] {
        let dragging = state.clone();
        run(&mut ui, async move {
            let _ = dragging
                .anchored_drag(MutatePriority::Default, move |scope, _| async move {
                    scope.drag_to(to, 0.0);
                })
                .await;
        });
        assert_eq!(state.current_value(), value);
        assert_eq!(state.require_offset(), to);
        assert_eq!(state.settled_value(), A, "not on an anchor");
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_customDrag_updatesVelocity
#[test]
fn a_custom_drag_tells_its_velocity() {
    let state = AnchoredDraggableState::with_anchors(A, abc());
    let mut ui = rule();
    let dragging = state.clone();
    run(&mut ui, async move {
        let _ = dragging
            .anchored_drag(MutatePriority::Default, |scope, _| async move {
                scope.drag_to(50.0, 123.0);
            })
            .await;
    });
    assert_eq!(state.last_velocity(), 123.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_customDrag_targetValueUpdate
#[test]
fn a_custom_drag_to_a_target_has_it_as_its_target() {
    let state =
        AnchoredDraggableState::with_anchors(A, anchors(&[(A, 0.0), (B, 200.0), (C, 300.0)]));
    let mut ui = rule();
    ui.set_auto_advance(false);
    let dragging = state.clone();
    let job = launch(async move {
        let _ = dragging
            .anchored_drag_to(C, MutatePriority::Default, |scope, _, _| async move {
                scope.drag_to(200.0, 0.0);
                std::future::pending::<()>().await;
            })
            .await;
    });
    ui.advance_time_by_frame();
    assert_eq!(state.target_value(), C);
    job.cancel();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_customDrag_settleOnInvalidState_shouldRespectConfirmValueChange
#[test]
fn a_custom_drag_doesnt_end_on_a_vetoed_value() {
    let blocked = Rc::new(Cell::new(false));
    let blocking = blocked.clone();
    let state = AnchoredDraggableState::with_anchors_and_confirm(
        B,
        anchors(&[(A, 0.0), (B, 200.0), (C, 300.0)]),
        move |value| !(blocking.get() && *value == C),
    );
    let mut ui = rule();
    let drag = |ui: &mut ComposeTestRule, to: f32| {
        let dragging = state.clone();
        run(ui, async move {
            let _ = dragging
                .anchored_drag(MutatePriority::Default, move |scope, _| async move {
                    scope.drag_to(to, 0.0);
                })
                .await;
        });
    };
    drag(&mut ui, 300.0);
    assert_eq!(state.current_value(), C);
    drag(&mut ui, 200.0);
    assert_eq!(state.current_value(), B);
    blocked.set(true);
    drag(&mut ui, 300.0);
    assert_ne!(state.current_value(), C);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_animateTo_alreadyAtTarget_noOps, anchoredDraggable_animateToWithDecay_alreadyAtTarget_noOps
#[test]
fn going_where_it_is_does_nothing() {
    let state = AnchoredDraggableState::with_anchors(B, abc());
    let mut ui = rule();
    let going = state.clone();
    let used = run(&mut ui, async move {
        going.animate_to(B).await.expect("nothing else");
        going
            .animate_to_with_decay(
                B,
                500.0,
                AnchoredDraggableDefaults::snap_animation_spec(),
                AnchoredDraggableDefaults::decay_animation_spec(),
            )
            .await
            .expect("nothing else")
    });
    assert_eq!(used, 0.0);
    assert_eq!(state.require_offset(), 100.0);
}

/// `A` and `B` 2000 px apart, from a part of the way, going to `B` with decay at `velocity`.
fn with_decay(velocity: f32) -> (AnchoredDraggableState<Value>, f32) {
    let state = AnchoredDraggableState::with_anchors(A, anchors(&[(A, 0.0), (B, 2000.0)]));
    state.dispatch_raw_delta(10.0);
    let mut ui = rule();
    let going = state.clone();
    let used = run(&mut ui, async move {
        going
            .animate_to_with_decay(
                B,
                velocity,
                compose_animation::tween(100),
                AnchoredDraggableDefaults::decay_animation_spec(),
            )
            .await
            .expect("nothing else")
    });
    (state, used)
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_animateToTarget_canDecayToTarget_positiveVelocity
#[test]
fn a_velocity_getting_there_decays_and_keeps_what_it_has_left() {
    // Decay reaches velocity / 4.2 by the exponential default: 8400 reach 2000 at once.
    let (state, used) = with_decay(20000.0);
    assert_eq!(state.current_value(), B);
    assert_eq!(state.require_offset(), 2000.0);
    assert!(used > 0.0 && used < 20000.0, "{used}");
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_animateToTarget_canNotDecayToTarget_positiveVelocity, anchoredDraggable_animateToTarget_zeroVelocity_usesSnapAnimationSpec, anchoredDraggable_dragNotInTheSameDirectionOfTarget_negativeOffset_positiveVelocity
#[test]
fn a_velocity_not_getting_there_or_going_away_animates_and_uses_it_all() {
    for velocity in [100.0, 0.0, -500.0] {
        let (state, used) = with_decay(velocity);
        assert_eq!(state.current_value(), B, "{velocity}");
        assert_eq!(used, velocity, "{velocity}");
    }
}

/// A box 200 wide with `A`, `B`, `C` at 0, 100, 200, no slop, by touch.
fn abc_box(
    start: Value,
    options: AnchoredDraggableOptions,
    vertical: bool,
) -> (ComposeTestRule, AnchoredDraggableState<Value>) {
    let state = AnchoredDraggableState::with_anchors(start, abc());
    let orientation = if vertical { Orientation::Vertical } else { Orientation::Horizontal };
    let mut ui = dragged(state.clone(), orientation, options);
    no_slop(&mut ui);
    (ui, state)
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableGestureTest.kt, anchoredDraggable_swipe_horizontal, anchoredDraggable_swipe_vertical
#[test]
fn a_swipe_goes_to_the_next_anchors() {
    for vertical in [false, true] {
        let (mut ui, state) = abc_box(A, AnchoredDraggableOptions::default(), vertical);
        let at = |main: f32| {
            if vertical { Offset::new(100.0, main) } else { Offset::new(main, 100.0) }
        };
        swipe(&mut ui, at(1.0), at(101.0));
        assert_eq!(state.current_value(), B);
        assert_eq!(state.require_offset(), 100.0);
        swipe(&mut ui, at(101.0), at(199.0));
        assert_eq!(state.current_value(), C);
        swipe(&mut ui, at(199.0), at(1.0));
        assert_eq!(state.current_value(), A);
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableGestureTest.kt, anchoredDraggable_swipe_disabled_horizontal, anchoredDraggable_swipe_disabled_vertical
#[test]
fn disabled_a_swipe_moves_nothing() {
    for vertical in [false, true] {
        let options =
            AnchoredDraggableOptions { enabled: false, ..AnchoredDraggableOptions::default() };
        let (mut ui, state) = abc_box(A, options, vertical);
        let at = |main: f32| {
            if vertical { Offset::new(100.0, main) } else { Offset::new(main, 100.0) }
        };
        swipe(&mut ui, at(1.0), at(199.0));
        assert_eq!(state.current_value(), A);
        assert_eq!(state.require_offset(), 0.0);
    }
}

/// Moves by `by` and lets go at `velocity`.
fn fling_box(ui: &mut ComposeTestRule, by: f32, velocity: f32) {
    let (from, to) = if by >= 0.0 { (10.0, 10.0 + by) } else { (10.0 - by, 10.0) };
    ui.on_root().perform_touch_input(|touch| {
        touch.swipe_with_velocity(Offset::new(from, 100.0), Offset::new(to, 100.0), velocity, None);
    });
    ui.wait_for_idle();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableGestureTest.kt, anchoredDraggable_velocityThreshold_swipe_velocityHigherThanThreshold_advances, anchoredDraggable_velocityThreshold_swipe_velocityLowerThanThreshold_doesntAdvance
#[test]
fn a_short_swipe_advances_only_past_the_velocity_threshold() {
    let (mut ui, state) = abc_box(A, AnchoredDraggableOptions::default(), false);
    fling_box(&mut ui, 20.0, 400.0);
    assert_eq!(state.current_value(), B, "over 125 px/s");
    let (mut ui, state) = abc_box(A, AnchoredDraggableOptions::default(), false);
    fling_box(&mut ui, 20.0, 60.0);
    assert_eq!(state.current_value(), A, "under it");
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableGestureTest.kt, anchoredDraggable_dragBeyondBounds_clampsAndSwipesBack
#[test]
fn a_drag_past_the_last_anchor_stays_there_and_comes_back_from_it() {
    let (mut ui, state) = abc_box(C, AnchoredDraggableOptions::default(), false);
    ui.on_root().perform_touch_input(|touch| {
        touch.down(Offset::new(100.0, 100.0));
        touch.move_by(Offset::new(100.0, 0.0));
    });
    ui.wait_for_idle();
    assert_eq!(state.require_offset(), 200.0);
    ui.on_root().perform_touch_input(|touch| {
        touch.move_by(Offset::new(-30.0, 0.0));
    });
    ui.wait_for_idle();
    assert_eq!(state.require_offset(), 170.0, "a move back moves at once");
    ui.on_root().perform_touch_input(|touch| touch.up());
    ui.wait_for_idle();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableGestureTest.kt, anchoredDraggable_startDragImmediately_default_processesWithoutSlopWhileAnimating, anchoredDraggable_startDragImmediately_false_animationNotCancelledByDrag
#[test]
fn while_it_animates_a_press_takes_it_at_once_unless_told_not_to() {
    for (immediately, stops) in [(None, true), (Some(false), false)] {
        let options = AnchoredDraggableOptions {
            start_drag_immediately: immediately,
            ..AnchoredDraggableOptions::default()
        };
        let (mut ui, state) = abc_box(A, options, false);
        ui.host().set_view_configuration(None);
        ui.set_auto_advance(false);
        ui.advance_time_by_frame();
        let animating = state.clone();
        let spec = compose_animation::tween(1000);
        launch(async move {
            let _ = animating.animate_to_with(C, spec).await;
        });
        ui.advance_time_by_frame();
        ui.advance_time_by_frame();
        assert!(state.is_animation_running());
        ui.on_root().perform_touch_input(|touch| touch.down(Offset::new(100.0, 100.0)));
        ui.advance_time_by_frame();
        assert_eq!(state.target_value() != C, stops, "{immediately:?}");
        ui.on_root().perform_touch_input(|touch| touch.up());
        ui.set_auto_advance(true);
        ui.wait_for_idle();
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableGestureTest.kt, anchoredDraggable_reverseDirection_true_reversesDeltas
#[test]
fn reversed_a_drag_goes_the_other_way() {
    let options = AnchoredDraggableOptions {
        reverse_direction: Some(true),
        ..AnchoredDraggableOptions::default()
    };
    let (mut ui, state) = abc_box(C, options, false);
    ui.on_root().perform_touch_input(|touch| {
        touch.down(Offset::new(10.0, 100.0));
        touch.move_by(Offset::new(30.0, 0.0));
    });
    ui.wait_for_idle();
    assert_eq!(state.require_offset(), 170.0);
    ui.on_root().perform_touch_input(|touch| touch.up());
    ui.wait_for_idle();
    // Its velocity too: a flick to the right goes to the lower anchor.
    let options = AnchoredDraggableOptions {
        reverse_direction: Some(true),
        ..AnchoredDraggableOptions::default()
    };
    let (mut ui, state) = abc_box(C, options, false);
    fling_box(&mut ui, 20.0, 400.0);
    assert_eq!(state.current_value(), B);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableGestureTest.kt, anchoredDraggable_reverseDirection_defaultValue_reversesDeltasInRTL
#[test]
fn right_to_left_a_horizontal_drag_goes_the_other_way_by_default() {
    let state = AnchoredDraggableState::with_anchors(A, abc());
    let direction = compose_runtime::mutable_state_of(LayoutDirection::Ltr);
    let read = direction.handle();
    let given = state.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
        let given = given.clone();
        compose_runtime::CompositionLocalProvider(
            LOCAL_LAYOUT_DIRECTION.provides(read.get()),
            move || {
                Box!(
                    modifier = Modifier.fill_max_size().anchored_draggable(
                        given.clone(),
                        Orientation::Horizontal,
                        AnchoredDraggableOptions::default()
                    )
                );
            },
        );
    });
    ui.wait_for_idle();
    no_slop(&mut ui);
    swipe(&mut ui, Offset::new(300.0, 100.0), Offset::new(400.0, 100.0));
    assert_eq!(state.current_value(), B);
    direction.set(LayoutDirection::Rtl);
    ui.wait_for_idle();
    swipe(&mut ui, Offset::new(400.0, 100.0), Offset::new(300.0, 100.0));
    assert_eq!(state.current_value(), C, "a move left goes forward");
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableGestureTest.kt, anchoredDraggable_fling_confirmValueChange_returnsFalse_returnsToSettledAnchor
#[test]
fn a_fling_to_a_vetoed_anchor_goes_back_to_the_settled_one() {
    let state = AnchoredDraggableState::with_anchors_and_confirm(A, abc(), |value| *value != B);
    let mut ui =
        dragged(state.clone(), Orientation::Horizontal, AnchoredDraggableOptions::default());
    no_slop(&mut ui);
    fling_box(&mut ui, 60.0, 400.0);
    assert_eq!(state.current_value(), A);
    assert_eq!(state.require_offset(), 0.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_fling_offsetPastHalfwayBetweenAnchors_beyondPosThreshold_advances, anchoredDraggable_fling_offsetLessThanHalfwayBetweenAnchors_beyondPosThreshold_advances, anchoredDraggable_fling_offsetLessHalfwayBetweenAnchors_beforePosThreshold_doesntAdvance, anchoredDraggable_fling_offsetPastHalfwayBetweenAnchors_beforePosThreshold_doesntAdvance
#[test]
fn a_slow_release_advances_by_the_positional_threshold() {
    // (the release's place in the way, the threshold's, whether it gets there)
    for (place, threshold, advances) in
        [(0.9, 0.8, true), (0.4, 0.3, true), (0.4, 0.5, false), (0.7, 0.8, false)]
    {
        let state = AnchoredDraggableState::with_anchors(A, anchors(&[(A, 0.0), (B, 200.0)]));
        let given = state.clone();
        let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
            let fling = remember_anchored_fling_behavior(
                given.clone(),
                Rc::new(move |distance| distance * threshold),
                AnchoredDraggableDefaults::snap_animation_spec(),
            );
            Box!(
                modifier = Modifier.size(200.dp()).anchored_draggable(
                    given.clone(),
                    Orientation::Horizontal,
                    AnchoredDraggableOptions {
                        fling_behavior: Some(fling),
                        ..AnchoredDraggableOptions::default()
                    }
                )
            );
        });
        ui.wait_for_idle();
        no_slop(&mut ui);
        ui.on_root().perform_touch_input(|touch| {
            touch.down(Offset::new(10.0, 100.0));
            for _ in 0..10 {
                touch.move_by(Offset::new(200.0 * place / 10.0, 0.0));
            }
            // A little velocity left, slower than the threshold.
            touch.move_pointer_by(0, Offset::new(0.0, 0.0), 30);
            touch.up();
        });
        ui.wait_for_idle();
        assert_eq!(state.current_value(), if advances { B } else { A }, "{place} {threshold}");
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_state_canSkipStateByFling
#[test]
fn a_long_fling_passes_an_anchor() {
    let (mut ui, state) = abc_box(A, AnchoredDraggableOptions::default(), false);
    fling_box(&mut ui, 160.0, 2000.0);
    assert_eq!(state.current_value(), C);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableGestureTest.kt, anchoredDraggable_updatesState
#[test]
fn a_new_state_takes_over() {
    let states = [
        AnchoredDraggableState::with_anchors(A, abc()),
        AnchoredDraggableState::with_anchors(A, abc()),
    ];
    let which = compose_runtime::mutable_state_of(0usize);
    let read = which.handle();
    let given = states.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
        Box!(
            modifier = Modifier.size(200.dp()).anchored_draggable(
                given[read.get()].clone(),
                Orientation::Horizontal,
                AnchoredDraggableOptions::default()
            )
        );
    });
    ui.wait_for_idle();
    no_slop(&mut ui);
    swipe(&mut ui, Offset::new(10.0, 100.0), Offset::new(110.0, 100.0));
    assert_eq!(states[0].current_value(), B);
    which.set(1);
    ui.wait_for_idle();
    swipe(&mut ui, Offset::new(10.0, 100.0), Offset::new(110.0, 100.0));
    assert_eq!(states[0].current_value(), B);
    assert_eq!(states[1].current_value(), B);
}

/// Lets go at `velocity` with the default fling: threshold half way, 125 px/s at a density of 1.
fn release(ui: &mut ComposeTestRule, state: &AnchoredDraggableState<Value>, velocity: f32) {
    let fling = anchored_fling_behavior(
        state.clone(),
        compose_ui::Density::new(1.0),
        Rc::new(AnchoredDraggableDefaults::positional_threshold),
        AnchoredDraggableDefaults::snap_animation_spec(),
    );
    let flinging = state.clone();
    run(ui, async move {
        let moving = flinging.clone();
        let scope = ScrollScope::new(move |pixels| moving.dispatch_raw_delta(pixels));
        fling.perform_fling(scope, velocity).await;
    });
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableGestureTest.kt, anchoredDraggable_velocityThreshold_settle_velocityHigherThanThreshold_advances, anchoredDraggable_velocityThreshold_settle_velocityLowerThanThreshold_doesntAdvance
#[test]
fn a_release_takes_the_next_anchor_past_half_way_or_past_the_velocity() {
    let mut ui = rule();
    let state = AnchoredDraggableState::with_anchors(A, abc());
    state.dispatch_raw_delta(60.0);
    release(&mut ui, &state, 126.0);
    assert_eq!(state.current_value(), B);
    let state = AnchoredDraggableState::with_anchors(A, abc());
    state.dispatch_raw_delta(40.0);
    release(&mut ui, &state, 125.0 * 0.9);
    assert_eq!(state.current_value(), A);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableGestureTest.kt, anchoredDraggable_dragAndSwipeBackWithVelocity_velocityHigherThanThreshold
#[test]
fn a_velocity_back_goes_back_from_near_the_anchor_left() {
    let mut ui = rule();
    let state = AnchoredDraggableState::with_anchors(B, anchors(&[(A, 0.0), (B, 200.0)]));
    state.dispatch_raw_delta(-60.0);
    assert_eq!(state.require_offset(), 140.0);
    release(&mut ui, &state, 126.0);
    assert_eq!(state.current_value(), B);
    let going = state.clone();
    run(&mut ui, async move {
        going.animate_to(A).await.expect("nothing else");
    });
    state.dispatch_raw_delta(60.0);
    release(&mut ui, &state, -125.0);
    assert_eq!(state.current_value(), A);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_performFling_positiveOffset_positiveVelocity, anchoredDraggable_performFling_positiveOffset_negativeVelocity
#[test]
fn under_both_thresholds_a_release_stays_either_way() {
    for (start, delta, velocity) in [(A, 90.0, 112.5), (B, -90.0, -112.5)] {
        let mut ui = rule();
        let state = AnchoredDraggableState::with_anchors(start, anchors(&[(A, 0.0), (B, 200.0)]));
        state.dispatch_raw_delta(delta);
        release(&mut ui, &state, velocity);
        assert_eq!(state.current_value(), start);
        assert_eq!(state.require_offset(), if start == A { 0.0 } else { 200.0 });
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_settleWhenOffsetEqualsTargetOffset
#[test]
fn a_release_on_an_anchor_stays_on_it() {
    let mut ui = rule();
    let state = AnchoredDraggableState::with_anchors(A, anchors(&[(A, 0.0), (B, 250.0)]));
    state.dispatch_raw_delta(250.0);
    release(&mut ui, &state, 1000.0);
    assert_eq!(state.require_offset(), 250.0);
    assert_eq!(state.current_value(), B);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableGestureTest.kt, anchoredDraggable_targetValue_animationCancelledResetsTargetValueToClosest
#[test]
fn a_cancelled_animation_targets_the_closest_anchor() {
    let state =
        AnchoredDraggableState::with_anchors(A, anchors(&[(A, 0.0), (B, 250.0), (C, 500.0)]));
    let mut ui = rule();
    ui.set_auto_advance(false);
    let animating = state.clone();
    launch(async move {
        let _ = animating.animate_to_with(C, compose_animation::tween(300)).await;
    });
    while state.require_offset() < 250.0 {
        ui.advance_time_by_frame();
    }
    assert_eq!(state.target_value(), C);
    let taking = state.clone();
    launch(async move {
        let _ = taking.anchored_drag(MutatePriority::Default, |_, _| async {}).await;
    });
    ui.advance_time_by_frame();
    assert_eq!(state.target_value(), B);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_dragCompletesExceptionally_cleansUp
#[test]
fn a_cancelled_drag_leaves_the_state_free() {
    let state = AnchoredDraggableState::with_anchors(A, abc());
    let mut ui = rule();
    let dragging = state.clone();
    let job = launch(async move {
        let _ = dragging
            .anchored_drag_to(C, MutatePriority::Default, |_, _, _| std::future::pending::<()>())
            .await;
    });
    ui.wait_for_idle();
    assert_eq!(state.target_value(), C);
    job.cancel();
    ui.wait_for_idle();
    assert_eq!(state.target_value(), A);
    let snapping = state.clone();
    run(&mut ui, async move {
        snapping.snap_to(B).await.expect("nothing holds it");
    });
    assert_eq!(state.current_value(), B);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_saver_restoresCurrentValue
#[test]
fn saved_and_restored_it_is_at_the_value_it_was() {
    let kept: Rc<RefCell<Option<AnchoredDraggableState<i32>>>> = Rc::default();
    let noted = kept.clone();
    let mut tester = StateRestorationTester::new();
    let mut ui = rule();
    tester.set_content(&mut ui, move || {
        let state =
            compose_runtime::remember_saveable_with(AnchoredDraggableState::<i32>::saver(), || {
                AnchoredDraggableState::new(3)
            });
        state.update_anchors(DraggableAnchors::new(|config| {
            config.at(1, 0.0);
            config.at(2, 100.0);
            config.at(3, 200.0);
        }));
        *noted.borrow_mut() = Some(state);
    });
    ui.wait_for_idle();
    tester.emulate_save_and_restore(&mut ui);
    let state = kept.borrow().clone().expect("composed");
    assert_eq!(state.current_value(), 3);
    let going = state.clone();
    run(&mut ui, async move {
        going.animate_to(2).await.expect("nothing else");
    });
    assert_eq!(state.current_value(), 2);
    tester.emulate_save_and_restore(&mut ui);
    let restored = kept.borrow().clone().expect("composed");
    assert_eq!(restored.current_value(), 2);
    assert_eq!(restored.require_offset(), 100.0);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableGestureTest.kt, anchoredDraggable_onDensityChanges_swipeWithVelocityHigherThanThreshold
#[test]
fn the_velocity_threshold_follows_the_density() {
    let state = AnchoredDraggableState::with_anchors(A, abc());
    let density = compose_runtime::mutable_state_of(1.0f32);
    let read = density.handle();
    let given = state.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), move || {
        let given = given.clone();
        compose_runtime::CompositionLocalProvider(
            compose_ui::LOCAL_DENSITY.provides(compose_ui::Density::new(read.get())),
            move || {
                Box!(
                    modifier = Modifier.fill_max_size().anchored_draggable(
                        given.clone(),
                        Orientation::Horizontal,
                        AnchoredDraggableOptions::default()
                    )
                );
            },
        );
    });
    ui.wait_for_idle();
    no_slop(&mut ui);
    fling_box(&mut ui, 20.0, 137.5);
    assert_eq!(state.current_value(), B, "over 125 px/s");
    density.set(2.0);
    ui.wait_for_idle();
    fling_box(&mut ui, 20.0, 137.5);
    assert_eq!(state.current_value(), B, "under 250 px/s");
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_targetState_updatedOnSwipe
#[test]
fn a_swipe_past_half_way_targets_the_next_anchor_before_it_ends() {
    let (mut ui, state) = abc_box(A, AnchoredDraggableOptions::default(), false);
    ui.on_root().perform_touch_input(|touch| {
        touch.down(Offset::new(10.0, 100.0));
        touch.move_by(Offset::new(20.0, 0.0));
    });
    ui.wait_for_idle();
    assert_eq!(state.target_value(), A);
    ui.on_root().perform_touch_input(|touch| {
        touch.move_by(Offset::new(40.0, 0.0));
    });
    ui.wait_for_idle();
    assert_eq!(state.target_value(), B);
    ui.on_root().perform_touch_input(|touch| touch.up());
    ui.wait_for_idle();
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_accessInInitialComposition_targetState, anchoredDraggable_progress_accessedInInitialComposition
#[test]
fn before_its_anchors_its_target_and_progress_can_be_read() {
    let seen: Rc<RefCell<Vec<(Value, f32)>>> = Rc::default();
    let noted = seen.clone();
    let mut ui = rule();
    ui.set_content(move || {
        let state = compose_runtime::remember(|| AnchoredDraggableState::new(B));
        noted.borrow_mut().push((state.target_value(), state.progress(&A, &B)));
    });
    ui.wait_for_idle();
    assert_eq!(seen.borrow()[0], (B, 1.0));
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_requireOffset_accessedInEffect_doesntThrow
#[test]
fn in_an_effect_the_offset_can_be_read_once_the_anchors_are_known() {
    let read: Rc<Cell<Option<f32>>> = Rc::default();
    let noted = read.clone();
    let mut ui = rule();
    ui.set_content(move || {
        let state = compose_runtime::remember(|| AnchoredDraggableState::with_anchors(B, abc()));
        let noted = noted.clone();
        compose_runtime::SideEffect(move || noted.set(Some(state.require_offset())));
    });
    ui.wait_for_idle();
    assert_eq!(read.get(), Some(100.0));
}

/// Goes through the values between `from` and `to` plus `overshoot`, in 100 ms, at any velocity.
struct LinearDecay {
    to: f32,
}

impl compose_animation::FloatDecayAnimationSpec for LinearDecay {
    fn abs_velocity_threshold(&self) -> f32 {
        0.0
    }
    fn value_from_nanos(&self, play_time: i64, initial_value: f32, _: f32) -> f32 {
        let fraction = (play_time as f32 / 100_000_000.0).min(1.0);
        initial_value + (self.to - initial_value) * fraction
    }
    fn duration_nanos(&self, _: f32, _: f32) -> i64 {
        100_000_000
    }
    fn velocity_from_nanos(&self, _: i64, initial_value: f32, _: f32) -> f32 {
        (self.to - initial_value) * 10.0
    }
    fn target_value(&self, _: f32, _: f32) -> f32 {
        self.to
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableDecayAnimationTest.kt, anchoredDraggable_animateToWithDecay
#[test]
fn a_decay_past_the_target_stops_on_it() {
    // (from, to, overshoot) either way, with and without passing the target.
    for (from, to, overshoot) in
        [(0.0, 500.0, 0.0), (0.0, 500.0, 100.0), (500.0, 0.0, 0.0), (500.0, 0.0, 100.0)]
    {
        let direction = if to > from { 1.0f32 } else { -1.0 };
        let state = AnchoredDraggableState::with_anchors(
            if from == 0.0 { A } else { B },
            anchors(&[(A, 0.0), (B, 500.0)]),
        );
        let target = if to == 0.0 { A } else { B };
        let mut ui = rule();
        ui.set_auto_advance(false);
        let going = state.clone();
        let decay = compose_animation::generate_decay_animation_spec(LinearDecay {
            to: to + overshoot * direction,
        });
        launch(async move {
            let _ = going
                .animate_to_with_decay(
                    target,
                    1250.0 * direction,
                    compose_animation::tween(1000),
                    decay,
                )
                .await;
        });
        let mut seen = Vec::new();
        for _ in 0..20 {
            ui.advance_time_by_frame();
            seen.push(state.require_offset());
        }
        assert!(seen[1] != from, "{from} {to} {overshoot}: it decays, quickly");
        assert!(
            seen.iter().all(|value| (value - from) * direction <= (to - from) * direction),
            "{from} {to} {overshoot}: {seen:?}"
        );
        assert_eq!(state.require_offset(), to);
        assert_eq!(state.current_value(), target);
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_animateToTarget_canDecayToTarget_negativeVelocity, anchoredDraggable_animateToTarget_canNotDecayToTarget_negativeVelocity, anchoredDraggable_dragNotInTheSameDirectionOfTarget_negativeOffset_negativeVelocity
#[test]
fn going_down_a_velocity_decays_or_animates_the_same() {
    for (velocity, decays) in [(-20000.0, true), (-100.0, false), (500.0, false)] {
        let state = AnchoredDraggableState::with_anchors(B, anchors(&[(A, 0.0), (B, 2000.0)]));
        state.dispatch_raw_delta(-10.0);
        let mut ui = rule();
        let going = state.clone();
        let used = run(&mut ui, async move {
            going
                .animate_to_with_decay(
                    A,
                    velocity,
                    compose_animation::tween(100),
                    AnchoredDraggableDefaults::decay_animation_spec(),
                )
                .await
                .expect("nothing else")
        });
        assert_eq!(state.current_value(), A);
        assert_eq!(state.require_offset(), 0.0);
        if decays {
            assert!(used < 0.0 && used > velocity, "{used}");
        } else {
            assert_eq!(used, velocity);
        }
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_customDrag_targetValueUpdate
#[test]
fn a_drag_to_a_target_ends_on_it_whatever_its_block_did() {
    let state = AnchoredDraggableState::with_anchors(A, abc());
    let mut ui = rule();
    let dragging = state.clone();
    run(&mut ui, async move {
        let _ = dragging.anchored_drag_to(C, MutatePriority::Default, |_, _, _| async {}).await;
    });
    assert_eq!(state.require_offset(), 200.0);
    assert_eq!(state.current_value(), C);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/anchoredDraggable/AnchoredDraggableStateTest.kt, anchoredDraggable_currentValue_updatedWithDeltaDispatch, testProgression
#[test]
fn every_pixel_of_a_drag_is_the_value_half_way_says() {
    let state = AnchoredDraggableState::with_anchors(A, anchors(&[(A, 0.0), (B, 20.0), (C, 40.0)]));
    let mut ui = rule();
    for (from, to) in [(A, B), (B, C)] {
        let dragging = state.clone();
        run(&mut ui, async move {
            let reading = dragging.clone();
            let _ = dragging
                .anchored_drag(MutatePriority::Default, move |scope, anchors| {
                    let dragging = reading.clone();
                    async move {
                        let origin = anchors.position_of(&from) as i32;
                        let destination = anchors.position_of(&to) as i32;
                        let distance = destination - origin;
                        for value in origin..=destination {
                            scope.drag_to(value as f32, 0.0);
                            let expected = if value < origin + distance / 2 { from } else { to };
                            assert_eq!(dragging.current_value(), expected, "at {value}");
                        }
                    }
                })
                .await;
            dragging.settle(AnchoredDraggableDefaults::snap_animation_spec()).await.expect("free");
        });
    }
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/AnchoredDraggable.kt, AnchoredDraggableState.settle(animationSpec)
#[test]
fn settling_goes_to_the_closest_anchor_or_back_from_a_veto() {
    let state = AnchoredDraggableState::with_anchors_and_confirm(A, abc(), |value| *value != C);
    let mut ui = rule();
    state.dispatch_raw_delta(60.0);
    let settling = state.clone();
    run(&mut ui, async move {
        settling.settle(compose_animation::tween(100)).await.expect("free");
    });
    assert_eq!((state.current_value(), state.require_offset()), (B, 100.0));
    assert_eq!(state.settled_value(), B);
    state.dispatch_raw_delta(70.0);
    let settling = state.clone();
    run(&mut ui, async move {
        settling.settle(compose_animation::tween(100)).await.expect("free");
    });
    assert_eq!((state.current_value(), state.require_offset()), (B, 100.0), "C is vetoed");
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/AnchoredDraggable.kt, AnchoredDraggableState.anchoredDrag(targetValue): if (confirmValueChange(targetValue))
#[test]
fn a_drag_to_a_vetoed_target_doesnt_end_as_it() {
    let state = AnchoredDraggableState::with_anchors_and_confirm(A, abc(), |value| *value != C);
    let mut ui = rule();
    let snapping = state.clone();
    run(&mut ui, async move {
        snapping.snap_to(C).await.expect("free");
    });
    assert_eq!(state.current_value(), A);
    assert_eq!(state.settled_value(), A);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/AnchoredDraggable.kt, animateToWithDecay: velocity - remainingVelocity
#[test]
fn a_decay_returns_the_velocity_it_used() {
    let state = AnchoredDraggableState::with_anchors(A, anchors(&[(A, 0.0), (B, 500.0)]));
    let mut ui = rule();
    let going = state.clone();
    let decay = compose_animation::generate_decay_animation_spec(LinearDecay { to: 600.0 });
    let used = run(&mut ui, async move {
        going
            .animate_to_with_decay(B, 1250.0, compose_animation::tween(100), decay)
            .await
            .expect("free")
    });
    // It goes at 6000 px/s by the fake, and stops on the target going that fast.
    assert_eq!(used, 1250.0 - 6000.0);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/snapping/SnapFlingBehavior.kt: performFling, withContext(motionScaleDuration)
#[test]
fn an_anchored_fling_runs_the_same_whatever_the_scale_around() {
    let mut frames_of = Vec::new();
    for ambient in [1.0, 0.0] {
        let state = AnchoredDraggableState::with_anchors(A, anchors(&[(A, 0.0), (B, 500.0)]));
        state.dispatch_raw_delta(300.0);
        let mut ui = rule();
        ui.set_auto_advance(false);
        let fling = anchored_fling_behavior(
            state.clone(),
            compose_ui::Density::new(1.0),
            Rc::new(AnchoredDraggableDefaults::positional_threshold),
            AnchoredDraggableDefaults::snap_animation_spec(),
        );
        let flinging = state.clone();
        let job = launch(async move {
            let moving = flinging.clone();
            let scope = ScrollScope::new(move |pixels| moving.dispatch_raw_delta(pixels));
            compose_ui::with_motion_duration_scale(ambient, fling.perform_fling(scope, 1000.0))
                .await;
        });
        let mut frames = 0;
        while job.is_active() {
            ui.advance_time_by_frame();
            frames += 1;
            assert!(frames < 500);
        }
        assert_eq!(state.require_offset(), 500.0);
        frames_of.push(frames);
    }
    assert!(frames_of[0] > 2, "{frames_of:?}");
    assert_eq!(frames_of[0], frames_of[1]);
}

/// Counts its wakes; an `Arc`'s strong count says what holds it.
#[derive(Default)]
struct CountedWake(std::sync::atomic::AtomicU32);

impl std::task::Wake for CountedWake {
    fn wake(self: std::sync::Arc<Self>) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/AnchoredDraggable.kt, restartable: a collector, one however often it's resumed
#[test]
fn a_drag_polled_again_holds_one_waker() {
    let state = AnchoredDraggableState::with_anchors(A, abc());
    let arc = std::sync::Arc::new(CountedWake::default());
    let waker = std::task::Waker::from(arc.clone());
    let mut context = std::task::Context::from_waker(&waker);
    let mut drag =
        Box::pin(state.anchored_drag(MutatePriority::Default, |_, _| std::future::pending::<()>()));
    assert!(drag.as_mut().poll(&mut context).is_pending());
    let held = std::sync::Arc::strong_count(&arc);
    for _ in 0..50 {
        assert!(drag.as_mut().poll(&mut context).is_pending());
    }
    assert_eq!(std::sync::Arc::strong_count(&arc), held);
    drop(drag);
    drop(waker);
    assert_eq!(std::sync::Arc::strong_count(&arc), 1, "none left");
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/AnchoredDraggable.kt, targetValue by derivedStateOf
#[test]
fn what_reads_the_target_recomposes_only_when_it_changes() {
    let state = AnchoredDraggableState::with_anchors(A, abc());
    let compositions = Rc::new(Cell::new(0));
    let (given, noted) = (state.clone(), compositions.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        given.target_value();
        noted.set(noted.get() + 1);
    });
    ui.wait_for_idle();
    assert_eq!(compositions.get(), 1);
    for _ in 0..20 {
        state.dispatch_raw_delta(1.0);
        ui.wait_for_idle();
    }
    assert_eq!(compositions.get(), 1, "still A");
    state.dispatch_raw_delta(40.0);
    ui.wait_for_idle();
    assert_eq!(state.target_value(), B);
    assert_eq!(compositions.get(), 2);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/AnchoredDraggable.kt, DefaultDraggableAnchors.positionOf: the first of an anchor given twice
#[test]
fn an_anchor_given_twice_is_at_the_first_place_and_counts_twice() {
    let anchors = anchors(&[(A, 0.0), (B, 100.0), (A, 300.0)]);
    assert_eq!(anchors.size(), 3);
    assert_eq!(anchors.position_of(&A), 0.0);
    assert_eq!(anchors.max_position(), 300.0);
    assert_eq!(anchors.closest_anchor(280.0), Some(A));
}
