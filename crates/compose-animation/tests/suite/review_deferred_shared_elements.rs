//! Review of `deferred-shared-elements` (D1340, D1341; D2100, D2101):
//! - The hand-off velocity, exactly: an offset velocity in px/s on each axis and sign against a
//!   `TargetBasedAnimation` from the preview's bounds (the bounds animation's first frame is the
//!   one after the hand-off), the scale velocity around an off-centre pivot on all four edges, and
//!   none for an element drawn in place in the content moved by hand (`containerAppliesTransforms`).
//! - A `SharedContentState` swapped on an attached node during a deferred phase (D2101), and an
//!   `AnimatedVisibility` reused between a regular and a deferred transition.
//! - Previews over several frames (no drift), an interrupted transition's deferred phase cancelled
//!   (held until its catch-up settles: D2100), and `DeferredAnimatedVisibility` pairing.

#![allow(non_snake_case)]

use std::rc::Rc;
use std::time::Duration;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Alignment, Color, IntOffset, IntSize, Modifier, Offset, Rect, TransformOrigin, UnitExt,
};

type Scope = AnimatedContentTransitionScope<&'static str>;

fn deferred_rule(
    side: i32,
    state: &DeferredTransitionState<&'static str>,
    spec: impl Fn(Scope) -> ContentTransform + 'static,
    mutable_spec: impl Fn(Scope) -> Option<MutableContentTransform> + 'static,
    content: impl Fn(&SharedTransitionScope, &AnimatedContentScope, &'static str) + 'static,
) -> ComposeTestRule {
    let held = state.clone();
    let (spec, mutable_spec, content) = (Rc::new(spec), Rc::new(mutable_spec), Rc::new(content));
    ComposeTestRule::sized(IntSize::new(side + 100, side + 100), move || {
        let (held, spec, mutable_spec, content) =
            (held.clone(), spec.clone(), mutable_spec.clone(), content.clone());
        let transition = remember_deferred_transition(&held, None);
        SharedTransitionLayout!(
            modifier = Modifier.size(side.dp()).background(Color::WHITE),
            move |scope: SharedTransitionScope| {
                let (spec, mutable_spec, content) =
                    (spec.clone(), mutable_spec.clone(), content.clone());
                transition.DeferredAnimatedContent(
                    Modifier,
                    move |it| spec(it),
                    Alignment::TOP_START,
                    ContentKey::state(),
                    move |it| mutable_spec(it),
                    move |it, target| content(&scope, &it, target),
                );
            }
        );
    })
}

fn origin(pivot_fraction_x: f32, pivot_fraction_y: f32) -> TransformOrigin {
    TransformOrigin { pivot_fraction_x, pivot_fraction_y }
}

fn bounds_in_root(ui: &mut ComposeTestRule, tag: &str) -> Rect {
    ui.on_node_with_tag(tag).fetch_semantics_node().bounds_in_root()
}

#[track_caller]
fn assert_offset_near(actual: Offset, expected: Offset, tolerance: f32, label: &str) {
    assert!(
        (actual.x - expected.x).abs() <= tolerance && (actual.y - expected.y).abs() <= tolerance,
        "{label}: expected {expected:?} within {tolerance}, got {actual:?}"
    );
}

#[track_caller]
fn assert_rect_near(actual: Rect, expected: Rect, tolerance: f32, label: &str) {
    assert!(
        (actual.left - expected.left).abs() <= tolerance
            && (actual.top - expected.top).abs() <= tolerance
            && (actual.right - expected.right).abs() <= tolerance
            && (actual.bottom - expected.bottom).abs() <= tolerance,
        "{label}: expected {expected:?} within {tolerance}, got {actual:?}"
    );
}

// ---- Hand-off velocity: exact `px/s` units, axes, signs, and chain rule ---------------------

/// Verifies that `offset_velocity_provider` (in `px/s`) feeds the shared element's `BoundsAnimation`
/// spring with the exact initial velocity vector `(vx, vy, vx, vy)` matching `TargetBasedAnimation`
/// on every edge, for asymmetric positive and negative X/Y velocities.
#[test]
fn handoff_offset_velocity_matches_px_per_second_spring_on_each_axis_and_sign() {
    let spring_spec = || spring_with(Spring::DAMPING_RATIO_NO_BOUNCY, Spring::STIFFNESS_LOW);

    for (vx, vy) in [(2400.0f32, -1800.0f32), (-2400.0f32, 1800.0f32)] {
        let state = DeferredTransitionState::new("B");
        let offset = mutable_state_of(IntOffset::new(20, 30));
        let (held, off_read) = (state.clone(), offset.handle());
        let initial_tx = MutableTransform::new(move |it, _| {
            if held.pending_target_state().is_some() {
                it.set_offset(off_read.get());
            }
        })
        .offset_velocity_provider(move || Offset::new(vx, vy));
        let content_tx = MutableContentTransform::of(initial_tx, MutableTransform::empty());

        let mut ui = deferred_rule(
            400,
            &state,
            |_| fade_in_with(tween(1000), 0.0).together_with(fade_out_with(tween(1000), 0.0)),
            move |_| Some(content_tx.clone()),
            move |scope, content, target| {
                let (scope, content) = (scope.clone(), content.clone());
                Box!(modifier = Modifier.fill_max_size(), move |_| {
                    let shared_state = scope.remember_shared_content_state("shared");
                    let options = SharedOptions {
                        bounds_transform: BoundsTransform::new(move |_, _| spring_spec().into()),
                        ..Default::default()
                    };
                    let (x, y, sz) = if target == "B" { (100, 100, 80) } else { (160, 140, 80) };
                    Box!(
                        modifier = Modifier
                            .offset(x.dp(), y.dp())
                            .then(scope.shared_element_with(&shared_state, &content, options))
                            .size(sz.dp())
                            .test_tag(format!("box-{target}"))
                    );
                });
            },
        );

        ui.wait_for_idle();
        ui.set_auto_advance(false);

        state.defer("A");
        ui.advance_time_by_frame();
        ui.wait_for_idle();

        // Preview bounds of "B" at (100, 100, 180, 180) shifted by (20, 30) -> (120, 130, 200, 210).
        let start_rect = Rect::new(120.0, 130.0, 200.0, 210.0);
        let end_rect = Rect::new(160.0, 140.0, 240.0, 220.0);
        let velocity_rect = Rect::new(vx, vy, vx, vy);
        let expected_anim =
            TargetBasedAnimation::with_velocity(spring_spec(), start_rect, end_rect, velocity_rect);

        assert_rect_near(bounds_in_root(&mut ui, "box-B"), start_rect, 0.5, "deferred preview");

        // Commit to "A": the hand-off frame starts the bounds animation from the preview, and its
        // first frame (play time 0, as an `Animatable`'s first `withFrameNanos`) is the next one.
        state.animate_to("A");
        ui.advance_time_by_frame();
        ui.wait_for_idle();
        assert_rect_near(bounds_in_root(&mut ui, "box-A"), start_rect, 0.5, "handoff frame");
        ui.advance_time_by_frame();
        ui.wait_for_idle();
        assert_rect_near(bounds_in_root(&mut ui, "box-A"), start_rect, 0.5, "play time 0");

        // Advance 1 frame (16 ms) and 3 frames (48 ms total): must match `TargetBasedAnimation`!
        ui.advance_time_by(Duration::from_millis(16));
        ui.wait_for_idle();
        assert_rect_near(
            bounds_in_root(&mut ui, "box-A"),
            expected_anim.value_from_millis(16),
            1.0,
            &format!("16ms with v=({vx}, {vy})"),
        );

        ui.advance_time_by(Duration::from_millis(32));
        ui.wait_for_idle();
        assert_rect_near(
            bounds_in_root(&mut ui, "box-A"),
            expected_anim.value_from_millis(48),
            1.0,
            &format!("48ms with v=({vx}, {vy})"),
        );
    }
}

/// Verifies the off-center pivot chain rule `v_edge = (current_edge - pivot) * v_scale + v_offset`
/// across all four edges: shrinking a `(50, 50, 150, 150)` element around `pivot = (300, 0)` pushes
/// its `left` edge RIGHT (`(150 - 300) * v_scale > 0`) even when `target` is at `(50, 50)` to the
/// left, while narrowing `right - left` (`(right - left) * v_scale < 0`).
#[test]
fn handoff_coupled_scale_and_offset_velocity_obeys_pivot_chain_rule_on_all_four_edges() {
    let run_case = |fast_shrink: bool| -> Rect {
        let state = DeferredTransitionState::new("B");
        let scale = mutable_state_of(1.0f32);
        let (held, s_read) = (state.clone(), scale.handle());
        let tx = MutableTransform::new(move |it, _| {
            if held.pending_target_state().is_some() {
                it.set_transform_origin(origin(1.0, 0.0));
                it.set_scale(s_read.get());
            }
        });
        let content_tx = MutableContentTransform::of(tx, MutableTransform::empty());

        let mut ui = deferred_rule(
            300,
            &state,
            |_| fade_in_with(tween(1000), 0.0).together_with(fade_out_with(tween(1000), 0.0)),
            move |_| Some(content_tx.clone()),
            |scope, content, target| {
                let (scope, content) = (scope.clone(), content.clone());
                Box!(modifier = Modifier.fill_max_size(), move |_| {
                    let shared_state = scope.remember_shared_content_state("shared");
                    let options = SharedOptions {
                        bounds_transform: BoundsTransform::new(|_, _| {
                            spring_with(Spring::DAMPING_RATIO_NO_BOUNCY, Spring::STIFFNESS_VERY_LOW)
                                .into()
                        }),
                        ..Default::default()
                    };
                    Box!(
                        modifier = Modifier
                            .offset(50.dp(), 50.dp())
                            .then(scope.shared_element_with(&shared_state, &content, options))
                            .size(100.dp())
                            .test_tag(format!("box-{target}"))
                    );
                });
            },
        );

        ui.wait_for_idle();
        ui.set_auto_advance(false);
        state.defer("A");
        ui.advance_time_by_frame();
        ui.wait_for_idle();

        if fast_shrink {
            for step in [0.9, 0.8, 0.7, 0.6] {
                ui.advance_time_by(Duration::from_millis(16));
                scale.set(step);
                ui.wait_for_idle();
            }
        } else {
            scale.set(0.6);
            ui.wait_for_idle();
            // Hold steady long enough (> 200 ms) so `VelocityTracker` reports zero scale velocity.
            for _ in 0..20 {
                ui.advance_time_by(Duration::from_millis(16));
                ui.wait_for_idle();
            }
        }

        // At scale = 0.6 around (300, 0): (50, 50, 150, 150) -> (150, 30, 210, 90).
        let preview = bounds_in_root(&mut ui, "box-B");
        assert_rect_near(preview, Rect::new(150.0, 30.0, 210.0, 90.0), 0.5, "preview at 0.6");

        state.animate_to("A");
        ui.advance_time_by_frame();
        ui.wait_for_idle();
        ui.advance_time_by(Duration::from_millis(32));
        ui.wait_for_idle();
        bounds_in_root(&mut ui, "box-A")
    };

    let with_vel = run_case(true);
    let zero_vel = run_case(false);

    assert!(
        with_vel.left > 150.0 && with_vel.left > zero_vel.left + 5.0,
        "expected positive left velocity from right-edge pivot: with_vel={with_vel:?}, zero_vel={zero_vel:?}"
    );
    assert!(
        with_vel.width() < 60.0 && with_vel.width() < zero_vel.width() - 3.0,
        "expected shrinking width from negative scale velocity: with_vel={with_vel:?}, zero_vel={zero_vel:?}"
    );
    assert!(
        with_vel.top < 30.0 && with_vel.top < zero_vel.top - 1.0,
        "expected upward top velocity from top-edge pivot: with_vel={with_vel:?}, zero_vel={zero_vel:?}"
    );
}

/// Drawn in place (`render_in_overlay_during_transition = false`) in the content moved by hand, a
/// shared element's hand-off takes no velocity: its container's layer carries the content's, as
/// Compose's `containerAppliesTransforms` (`forcedHandoffVelocity = null`). Drawn in the overlay,
/// it takes the gesture's.
#[test]
fn render_in_overlay_false_does_not_double_apply_container_velocity_at_handoff() {
    let run_mode = |in_overlay: bool, velocity: Offset| -> Rect {
        let state = DeferredTransitionState::new("B");
        let off = mutable_state_of(IntOffset::new(10, 20));
        let (held, off_read) = (state.clone(), off.handle());
        let tx = MutableTransform::new(move |it, _| {
            if held.pending_target_state().is_some() {
                it.set_offset(off_read.get());
            }
        })
        .offset_velocity_provider(move || velocity);
        let content_tx = MutableContentTransform::of(tx, MutableTransform::empty());

        let mut ui = deferred_rule(
            400,
            &state,
            |_| {
                let spec = || spring_with(Spring::DAMPING_RATIO_NO_BOUNCY, Spring::STIFFNESS_LOW);
                fade_in_with(tween(1000), 0.0)
                    .together_with(slide_out_with(spec(), |_| IntOffset::new(60, 40)))
            },
            move |_| Some(content_tx.clone()),
            move |scope, content, target| {
                let (scope, content) = (scope.clone(), content.clone());
                Box!(modifier = Modifier.fill_max_size(), move |_| {
                    let shared_state = scope.remember_shared_content_state("shared");
                    let options = SharedOptions {
                        render_in_overlay_during_transition: in_overlay,
                        bounds_transform: BoundsTransform::new(|_, _| {
                            spring_with(Spring::DAMPING_RATIO_NO_BOUNCY, Spring::STIFFNESS_LOW)
                                .into()
                        }),
                        ..Default::default()
                    };
                    let (x, y) = if target == "B" { (80, 80) } else { (140, 120) };
                    Box!(
                        modifier = Modifier
                            .offset(x.dp(), y.dp())
                            .then(scope.shared_bounds_with(&shared_state, &content, options))
                            .size(60.dp())
                            .test_tag(format!("box-{target}"))
                    );
                });
            },
        );

        ui.wait_for_idle();
        ui.set_auto_advance(false);
        state.defer("A");
        ui.advance_time_by_frame();
        ui.wait_for_idle();
        // At (80, 80) moved by (10, 20).
        assert_rect_near(
            bounds_in_root(&mut ui, "box-B"),
            Rect::new(90.0, 100.0, 150.0, 160.0),
            0.5,
            "preview",
        );

        state.animate_to("A");
        ui.advance_time_by_frame();
        ui.wait_for_idle();
        ui.advance_time_by(Duration::from_millis(48));
        ui.wait_for_idle();
        bounds_in_root(&mut ui, "box-B")
    };

    let fling = Offset::new(1800.0, -1200.0);
    let in_place = run_mode(false, fling);
    let in_place_still = run_mode(false, Offset::ZERO);
    assert_rect_near(in_place, in_place_still, 0.5, "in place, the velocity isn't the element's");
    let overlay = run_mode(true, fling);
    let overlay_still = run_mode(true, Offset::ZERO);
    assert_rect_near(overlay_still, in_place_still, 0.5, "without velocity, both alike");
    assert!(
        overlay.left > overlay_still.left + 10.0 && overlay.top < overlay_still.top - 10.0,
        "in the overlay, the element takes the velocity: {overlay:?} vs {overlay_still:?}"
    );
}

// ---- A `SharedContentState` swapped on an attached `SharedBoundsNode` (D2101) ---------------

/// Another `SharedContentState` given to an attached `shared_bounds`: the old key's match goes,
/// the new key matches its peer, follows the deferred transform and hands off from it. (D2101's
/// missing setup healed at the next change the old entry observed, so this passes without it.)
#[test]
fn swapping_shared_content_state_key_in_place_during_deferred_transition_matches_new_peer() {
    let state = DeferredTransitionState::new("B");
    let use_k2 = mutable_state_of(false);
    let scale = mutable_state_of(1.0f32);
    let (held, k2_read, s_read) = (state.clone(), use_k2.handle(), scale.handle());
    // A's states, read after the frame (read in A's composition, they'd wait for a recomposition).
    let k1_peer = Rc::new(std::cell::RefCell::new(None::<SharedContentState>));
    let k2_peer = Rc::new(std::cell::RefCell::new(None::<SharedContentState>));
    let (k1_obs, k2_obs) = (k1_peer.clone(), k2_peer.clone());

    let mut ui = deferred_rule(
        300,
        &state,
        |_| fade_in_with(tween(300), 0.0).together_with(fade_out_with(tween(300), 0.0)),
        move |_| {
            let held = held.clone();
            Some(MutableContentTransform::new().initial_content_transform(move |it, _| {
                if held.pending_target_state().is_some() {
                    it.set_transform_origin(origin(0.0, 0.0));
                    it.set_scale(s_read.get());
                }
            }))
        },
        move |scope, content, target| {
            let (scope, content) = (scope.clone(), content.clone());
            let (k1_obs, k2_obs) = (k1_obs.clone(), k2_obs.clone());
            Box!(modifier = Modifier.fill_max_size(), move |_| {
                let s1 = scope.remember_shared_content_state("k1");
                let s2 = scope.remember_shared_content_state("k2");
                if target == "A" {
                    *k1_obs.borrow_mut() = Some(s1.clone());
                    *k2_obs.borrow_mut() = Some(s2.clone());
                    Box!(
                        modifier =
                            scope.shared_bounds(&s1, &content).size(60.dp()).test_tag("peer-k1")
                    );
                    Box!(
                        modifier = Modifier
                            .offset(140.dp(), 160.dp())
                            .then(scope.shared_bounds(&s2, &content))
                            .size(80.dp())
                            .test_tag("peer-k2")
                    );
                } else {
                    let chosen = if k2_read.get() { &s2 } else { &s1 };
                    Box!(
                        modifier = scope
                            .shared_bounds(chosen, &content)
                            .size(100.dp())
                            .test_tag("dynamic-B")
                    );
                }
            });
        },
    );

    ui.wait_for_idle();
    use_k2.set(true);
    ui.wait_for_idle();

    ui.set_auto_advance(false);
    state.defer("A");
    scale.set(0.5);
    ui.advance_time_by_frame();
    ui.wait_for_idle();

    let matched = |peer: &Rc<std::cell::RefCell<Option<SharedContentState>>>| {
        peer.borrow().as_ref().expect("A composed").is_match_found()
    };
    assert!(!matched(&k1_peer), "old key k1 must no longer report a match");
    assert!(matched(&k2_peer), "new key k2 must report a match");
    assert_rect_near(
        bounds_in_root(&mut ui, "dynamic-B"),
        Rect::new(0.0, 0.0, 50.0, 50.0),
        0.5,
        "dynamic-B scaled during deferred preview",
    );

    state.animate_to("A");
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_rect_near(
        bounds_in_root(&mut ui, "peer-k2"),
        Rect::new(0.0, 0.0, 50.0, 50.0),
        1.0,
        "peer-k2 starts at dynamic-B's previewed bounds at hand-off",
    );

    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_rect_near(
        bounds_in_root(&mut ui, "peer-k2"),
        Rect::new(140.0, 160.0, 220.0, 240.0),
        0.5,
        "peer-k2 settles at target bounds",
    );
}

// ---- An `AnimatedVisibility` reused between a regular and a deferred transition ------------

/// `ReusableContent` switching a slot from a regular `AnimatedVisibility` to a deferred one and
/// back: the deferred one's shared element follows its transform, and the regular one's isn't
/// left scaled. (The branches are separate groups, so the nodes aren't reused across them: an
/// `EnterExitNode`'s kinds, fixed when made, never meet a transition of the other kind.)
#[test]
fn reusing_enter_exit_between_regular_and_deferred_transitions_updates_transform_local() {
    let slot_key = mutable_state_of(0i32);
    let deferred_state = DeferredTransitionState::new(true);
    let scale = mutable_state_of(0.5f32);
    let (k_read, held, s_read) = (slot_key.handle(), deferred_state.clone(), scale.handle());

    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let (held, current_key) = (held.clone(), k_read.get());
        SharedTransitionLayout!(
            modifier = Modifier.size(200.dp()),
            move |scope: SharedTransitionScope| {
                let held = held.clone();
                let deferred_tx = remember_deferred_transition(&held, None);
                let scope_peer = scope.clone();
                deferred_tx.DeferredAnimatedVisibility(
                    |visible| !*visible,
                    Modifier,
                    Some(EnterTransition::NONE),
                    Some(ExitTransition::NONE),
                    None,
                    move |vis| {
                        let shared = scope_peer.remember_shared_content_state("item");
                        Box!(
                            modifier = scope_peer
                                .shared_element(&shared, &vis)
                                .size(40.dp())
                                .test_tag("peer-false")
                        );
                    },
                );

                ReusableContent(current_key, move || {
                    let scope_inner = scope.clone();
                    if current_key == 1 {
                        let held_inner = held.clone();
                        let tx = MutableTransform::new(move |it, _| {
                            if held_inner.pending_target_state().is_some() {
                                it.set_transform_origin(origin(0.0, 0.0));
                                it.set_scale(s_read.get());
                            }
                        });
                        deferred_tx.DeferredAnimatedVisibility(
                            |visible| *visible,
                            Modifier.fill_max_size(),
                            Some(EnterTransition::NONE),
                            Some(ExitTransition::NONE),
                            Some(tx),
                            move |vis| {
                                let scope_inner = scope_inner.clone();
                                Box!(modifier = Modifier.fill_max_size(), move |_| {
                                    let shared = scope_inner.remember_shared_content_state("item");
                                    Box!(
                                        modifier = scope_inner
                                            .shared_element(&shared, &vis)
                                            .size(100.dp())
                                            .test_tag("reused-box")
                                    );
                                });
                            },
                        );
                    } else {
                        let regular = update_transition(true, Some("regular"));
                        regular.AnimatedVisibility(
                            |visible| *visible,
                            Modifier.fill_max_size(),
                            EnterTransition::NONE,
                            ExitTransition::NONE,
                            move |vis| {
                                let scope_inner = scope_inner.clone();
                                Box!(modifier = Modifier.fill_max_size(), move |_| {
                                    let shared = scope_inner.remember_shared_content_state("item");
                                    Box!(
                                        modifier = scope_inner
                                            .shared_element(&shared, &vis)
                                            .size(100.dp())
                                            .test_tag("reused-box")
                                    );
                                });
                            },
                        );
                    }
                });
            }
        );
    });

    ui.wait_for_idle();
    assert_rect_near(
        bounds_in_root(&mut ui, "reused-box"),
        Rect::new(0.0, 0.0, 100.0, 100.0),
        0.5,
        "regular transition initial",
    );

    slot_key.set(1);
    ui.wait_for_idle();
    deferred_state.defer(false);
    ui.wait_for_idle();
    assert_rect_near(
        bounds_in_root(&mut ui, "reused-box"),
        Rect::new(0.0, 0.0, 50.0, 50.0),
        0.5,
        "reused into deferred transition with scale 0.5",
    );

    slot_key.set(2);
    ui.wait_for_idle();
    assert_rect_near(
        bounds_in_root(&mut ui, "reused-box"),
        Rect::new(0.0, 0.0, 100.0, 100.0),
        0.5,
        "reused back into regular transition without leaked scale",
    );
}

// ---- Multi-step preview, mid-flight cancelled interruption, & `DeferredAnimatedVisibility` --

/// Stepping `(scale, offset)` across 5 distinct preview frames around an off-center origin
/// `(0.25, 0.75)` produces exact transformed bounds on every frame with zero cumulative drift; and
/// interrupting a running `"A" -> "B"` transition with `defer("A")` and then cancelling back to
/// `"B"` (`animate_to("B")`) once the catch-up settled hands off without a jump (D2100).
#[test]
fn multi_step_preview_has_zero_drift_and_cancelled_interruption_hands_off_without_jump() {
    let state = DeferredTransitionState::new("A");
    let scale = mutable_state_of(1.0f32);
    let offset = mutable_state_of(IntOffset::ZERO);
    let (held, s_read, o_read) = (state.clone(), scale.handle(), offset.handle());

    let mut ui = deferred_rule(
        400,
        &state,
        |_| {
            fade_in_with(tween(600).easing(LINEAR_EASING), 0.0)
                .together_with(fade_out_with(tween(600).easing(LINEAR_EASING), 0.0))
        },
        move |_| {
            let held = held.clone();
            Some(MutableContentTransform::new().initial_content_transform(move |it, _| {
                if held.pending_target_state().is_some() {
                    it.set_transform_origin(origin(0.25, 0.75));
                    it.set_scale(s_read.get());
                    it.set_offset(o_read.get());
                }
            }))
        },
        |scope, content, target| {
            let (scope, content) = (scope.clone(), content.clone());
            Box!(modifier = Modifier.fill_max_size(), move |_| {
                let shared = scope.remember_shared_content_state("card");
                let options = SharedOptions {
                    bounds_transform: BoundsTransform::new(|_, _| {
                        tween(600).easing(LINEAR_EASING).into()
                    }),
                    ..Default::default()
                };
                let (x, y, sz) = if target == "A" { (40, 60, 120) } else { (200, 180, 80) };
                Box!(
                    modifier = Modifier
                        .offset(x.dp(), y.dp())
                        .then(scope.shared_bounds_with(&shared, &content, options))
                        .size(sz.dp())
                        .test_tag(format!("card-{target}"))
                );
            });
        },
    );

    ui.wait_for_idle();
    ui.set_auto_advance(false);

    state.defer("B");
    let steps = [
        (0.9f32, IntOffset::new(5, -4)),
        (0.8f32, IntOffset::new(12, -10)),
        (0.65f32, IntOffset::new(-8, 15)),
        (0.5f32, IntOffset::new(20, -25)),
        (0.75f32, IntOffset::new(-15, 10)),
    ];
    for (step_scale, step_off) in steps {
        scale.set(step_scale);
        offset.set(step_off);
        ui.advance_time_by_frame();
        ui.wait_for_idle();

        let pivot = Offset::new(100.0, 300.0);
        let base = Rect::new(40.0, 60.0, 160.0, 180.0);
        let expected_top_left = pivot
            + (base.top_left() - pivot) * step_scale
            + Offset::new(step_off.x as f32, step_off.y as f32);
        let expected = Rect::from_offset_size(expected_top_left, base.size() * step_scale);
        assert_rect_near(bounds_in_root(&mut ui, "card-A"), expected, 0.5, "multi-step preview");
    }

    state.animate_to("A");
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    scale.set(1.0);
    offset.set(IntOffset::ZERO);
    ui.wait_for_idle();

    ui.set_auto_advance(false);
    state.animate_to("B");
    ui.advance_time_by(Duration::from_millis(300));
    ui.wait_for_idle();

    state.defer("A");
    scale.set(0.8);
    offset.set(IntOffset::new(24, -18));
    // Held until the catch-up has settled: a hand-off during it starts from the pivot of the
    // catching-up origin (`lastTransformOrigin`) while the overlay draws at the manual one, a jump
    // Compose has too (D2100).
    for _ in 0..40 {
        ui.advance_time_by_frame();
        ui.wait_for_idle();
    }
    let before_cancel = bounds_in_root(&mut ui, "card-B");

    state.animate_to("B");
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    let at_cancel_handoff = bounds_in_root(&mut ui, "card-B");
    // Its top left: drawn in the overlay at its bounds, it's placed so (through its container's
    // layer, still scaled at the hand-off) its top left is theirs; its size in the root is scaled
    // by that layer again, as Compose's (the skia suite checks the pixels).
    assert_offset_near(
        at_cancel_handoff.top_left(),
        before_cancel.top_left(),
        1.5,
        "no jump when cancelling interrupted gesture",
    );

    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_rect_near(
        bounds_in_root(&mut ui, "card-B"),
        Rect::new(200.0, 180.0, 280.0, 260.0),
        0.5,
        "settled at B after cancelled interruption",
    );
}

/// Two sibling `DeferredAnimatedVisibility` blocks over a `DeferredTransitionState<bool>` pair
/// their `shared_element`s cleanly across `defer`, preview transform, hand-off, and completion.
#[test]
fn deferred_animated_visibility_pairs_shared_elements_across_boolean_states() {
    let state = DeferredTransitionState::new(false);
    let scale = mutable_state_of(1.0f32);
    let offset = mutable_state_of(IntOffset::ZERO);
    let (held, s_read, o_read) = (state.clone(), scale.handle(), offset.handle());

    let mut ui = ComposeTestRule::sized(IntSize::new(350, 350), move || {
        let held = held.clone();
        let transition = remember_deferred_transition(&held, None);
        SharedTransitionLayout!(
            modifier = Modifier.size(300.dp()),
            move |scope: SharedTransitionScope| {
                let held_out = held.clone();
                let tx_out = remember(move || {
                    MutableTransform::new(move |it, _| {
                        if held_out.pending_target_state() == Some(true) {
                            it.set_transform_origin(origin(0.5, 0.5));
                            it.set_scale(s_read.get());
                            it.set_offset(o_read.get());
                        }
                    })
                });
                let scope_a = scope.clone();
                transition.DeferredAnimatedVisibility(
                    |visible| !*visible,
                    Modifier.fill_max_size(),
                    Some(fade_in_with(tween(300), 0.0)),
                    Some(fade_out_with(tween(300), 0.0)),
                    Some(tx_out),
                    move |vis| {
                        // A box: the content's fixed size doesn't stretch the pill.
                        let scope_a = scope_a.clone();
                        Box!(modifier = Modifier.fill_max_size(), move |_| {
                            let shared = scope_a.remember_shared_content_state("pill");
                            Box!(
                                modifier = Modifier
                                    .offset(30.dp(), 50.dp())
                                    .then(scope_a.shared_element(&shared, &vis))
                                    .size(100.dp())
                                    .test_tag("pill-collapsed")
                            );
                        });
                    },
                );

                let scope_b = scope.clone();
                transition.DeferredAnimatedVisibility(
                    |visible| *visible,
                    Modifier.fill_max_size(),
                    Some(fade_in_with(tween(300), 0.0)),
                    Some(fade_out_with(tween(300), 0.0)),
                    None,
                    move |vis| {
                        let scope_b = scope_b.clone();
                        Box!(modifier = Modifier.fill_max_size(), move |_| {
                            let shared = scope_b.remember_shared_content_state("pill");
                            Box!(
                                modifier = Modifier
                                    .offset(160.dp(), 180.dp())
                                    .then(scope_b.shared_element(&shared, &vis))
                                    .size(80.dp())
                                    .test_tag("pill-expanded")
                            );
                        });
                    },
                );
            }
        );
    });

    ui.wait_for_idle();
    ui.set_auto_advance(false);

    state.defer(true);
    scale.set(0.5);
    offset.set(IntOffset::new(15, -10));
    ui.advance_time_by_frame();
    ui.wait_for_idle();

    let expected_preview = Rect::new(105.0, 90.0, 155.0, 140.0);
    assert_rect_near(
        bounds_in_root(&mut ui, "pill-collapsed"),
        expected_preview,
        0.5,
        "collapsed pill during deferred visibility preview",
    );

    state.animate_to(true);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_rect_near(
        bounds_in_root(&mut ui, "pill-expanded"),
        expected_preview,
        1.0,
        "expanded pill at hand-off",
    );

    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_rect_near(
        bounds_in_root(&mut ui, "pill-expanded"),
        Rect::new(160.0, 180.0, 240.0, 260.0),
        0.5,
        "expanded pill at rest",
    );
}
