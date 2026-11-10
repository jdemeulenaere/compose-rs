//! Pixel review of `deferred-shared-elements` (D1340, D1341, D2100):
//! - `OverlayClip` path coordinates when incoming and outgoing `shared_bounds` have distinct
//!   offsets inside `SharedTransitionLayout` during deferred scale + offset (verifying `D1341`'s
//!   `content_offset` adjustment does not shift the overlay clip path).
//! - `render_in_shared_transition_scope_overlay_with` combined with `Modifier.offset` both before
//!   and after the overlay modifier, drawn above a lower-`z_index` shared element during a
//!   deferred preview and active transition (`D1341` check).
//! - An interrupted transition's deferred phase with an off-centre scale and an offset, cancelled
//!   back to the previous target once its catch-up settled, without a jump (D2100).

use std::rc::Rc;
use std::time::Duration;

use compose::animation::*;
use compose::foundation::layout::*;
use compose::runtime::*;
use compose::test::*;
use compose::ui::{
    Alignment, Color, IntOffset, IntRect, IntSize, Modifier, Path, Rect, TransformOrigin, UnitExt,
};

const TAG: &str = "scope";

type Scope = AnimatedContentTransitionScope<&'static str>;

fn deferred_rule(
    side: i32,
    state: &DeferredTransitionState<&'static str>,
    spec: impl Fn(Scope) -> ContentTransform + 'static,
    mutable_spec: impl Fn(Scope) -> Option<MutableContentTransform> + 'static,
    content: impl Fn(&SharedTransitionScope, &AnimatedContentScope, &'static str) + 'static,
) -> ComposeTestRule {
    set_capture_renderer(compose_skia::raster_colors);
    let held = state.clone();
    let (spec, mutable_spec, content) = (Rc::new(spec), Rc::new(mutable_spec), Rc::new(content));
    ComposeTestRule::sized(IntSize::new(side + 100, side + 100), move || {
        let (held, spec, mutable_spec, content) =
            (held.clone(), spec.clone(), mutable_spec.clone(), content.clone());
        SharedTransitionLayout!(
            modifier = Modifier.size(side.dp()).test_tag(TAG).background(Color::WHITE),
            move |scope: SharedTransitionScope| {
                let transition = remember_deferred_transition(&held, None);
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

fn bounds_of(ui: &mut ComposeTestRule, matches: impl Fn(Color) -> bool) -> IntRect {
    let image = ui.on_node_with_tag(TAG).capture_to_image();
    let (mut left, mut top, mut right, mut bottom) = (image.width(), image.height(), -1, -1);
    for y in 0..image.height() {
        for x in 0..image.width() {
            if matches(image.pixel(x, y)) {
                left = left.min(x);
                right = right.max(x);
                top = top.min(y);
                bottom = bottom.max(y);
            }
        }
    }
    if right == -1 { IntRect::ZERO } else { IntRect::new(left, top, right + 1, bottom + 1) }
}

fn near(a: IntRect, b: IntRect, tolerance: i32) -> bool {
    (a.left - b.left).abs() <= tolerance
        && (a.top - b.top).abs() <= tolerance
        && (a.right - b.right).abs() <= tolerance
        && (a.bottom - b.bottom).abs() <= tolerance
}

/// When outgoing `"B"` (`(40, 60)`, `120 x 120`) and incoming `"A"` (`(180, 140)`, `80 x 80`) sit
/// at different offsets and clip to the top-left quadrant of `bounds` via `OverlayClip`, scaling
/// `"B"` by `0.5` around `(150, 150)` plus `offset = (20, -10)` places `"B"` at
/// `(115, 95, 175, 155)`, and the overlay clip restricts drawn red pixels to `(115, 95, 145, 125)`.
#[test]
fn overlay_clip_clips_at_transformed_bounds_when_incoming_and_outgoing_offsets_differ() {
    let state = DeferredTransitionState::new("B");
    let scale = mutable_state_of(1.0f32);
    let offset = mutable_state_of(IntOffset::ZERO);
    let (held, s_read, o_read) = (state.clone(), scale.handle(), offset.handle());

    let transform = MutableContentTransform::new().initial_content_transform(move |it, _| {
        if held.pending_target_state().is_some() {
            it.set_transform_origin(origin(0.5, 0.5));
            it.set_scale(s_read.get());
            it.set_offset(o_read.get());
        }
    });

    let mut ui = deferred_rule(
        300,
        &state,
        |_| fade_in_with(tween(400), 0.0).together_with(fade_out_with(tween(400), 0.0)),
        move |_| Some(transform.clone()),
        |scope, content, target| {
            let (scope, content) = (scope.clone(), content.clone());
            Box!(modifier = Modifier.fill_max_size(), move |_| {
                let shared = scope.remember_shared_content_state("clipped");
                let quadrant_clip = OverlayClip::new(|_, bounds: Rect, _, _| {
                    let path = Path::new();
                    let center = bounds.center();
                    path.add_rect(Rect::new(bounds.left, bounds.top, center.x, center.y));
                    Some(path)
                });
                let options = SharedOptions {
                    clip_in_overlay_during_transition: quadrant_clip,
                    ..Default::default()
                };
                let (x, y, sz) = if target == "B" { (40, 60, 120) } else { (180, 140, 80) };
                Box!(
                    modifier = Modifier
                        .offset(x.dp(), y.dp())
                        .then(scope.shared_bounds_with(&shared, &content, options))
                        .size(sz.dp())
                        .background(Color::RED)
                );
            });
        },
    );

    ui.wait_for_idle();
    state.defer("A");
    scale.set(0.5);
    offset.set(IntOffset::new(20, -10));
    ui.wait_for_idle();

    let red = bounds_of(&mut ui, |c| c.red() > 0.9 && c.green() < 0.1 && c.blue() < 0.1);
    assert_eq!(red, IntRect::new(115, 95, 145, 125));
}

/// `render_in_shared_transition_scope_overlay_with` draws at the exact offset in scope coordinates
/// regardless of whether `Modifier.offset` appears before or after it, and renders above a
/// `z_index = 0.0` shared element during both the deferred phase and the active transition (D1341).
#[test]
fn render_in_shared_transition_scope_overlay_draws_at_offset_before_and_after_modifier() {
    for offset_before in [true, false] {
        let state = DeferredTransitionState::new("B");
        let scale = mutable_state_of(0.5f32);
        let (held, s_read) = (state.clone(), scale.handle());
        let transform = MutableContentTransform::new().initial_content_transform(move |it, _| {
            if held.pending_target_state().is_some() {
                it.set_transform_origin(origin(0.0, 0.0));
                it.set_scale(s_read.get());
            }
        });

        let mut ui = deferred_rule(
            300,
            &state,
            |_| fade_in_with(tween(500), 0.0).together_with(fade_out_with(tween(500), 0.0)),
            move |_| Some(transform.clone()),
            move |scope, content, target| {
                let (scope, content) = (scope.clone(), content.clone());
                Box!(modifier = Modifier.fill_max_size(), move |_| {
                    let shared = scope.remember_shared_content_state("under");
                    let sz = if target == "B" { 200 } else { 120 };
                    Box!(
                        modifier = scope
                            .shared_bounds(&shared, &content)
                            .size(sz.dp())
                            .background(Color::RED)
                    );

                    if target == "A" {
                        let scope_active = scope.clone();
                        let overlay = scope
                            .render_in_shared_transition_scope_overlay_with(2.0, move || {
                                scope_active.is_transition_active()
                            });
                        let badge_mod = if offset_before {
                            Modifier.offset(30.dp(), 50.dp()).then(overlay)
                        } else {
                            overlay.offset(30.dp(), 50.dp())
                        };
                        Box!(modifier = badge_mod.size(40.dp()).background(Color::BLUE));
                    }
                });
            },
        );

        ui.wait_for_idle();
        ui.set_auto_advance(false);

        state.defer("A");
        ui.advance_time_by_frame();
        ui.wait_for_idle();
        let blue_deferred =
            bounds_of(&mut ui, |c| c.blue() > 0.9 && c.red() < 0.1 && c.green() < 0.1);
        assert_eq!(
            blue_deferred,
            IntRect::new(30, 50, 70, 90),
            "offset_before={offset_before} during deferred phase"
        );

        state.animate_to("A");
        ui.advance_time_by(Duration::from_millis(100));
        ui.wait_for_idle();
        let blue_mid = bounds_of(&mut ui, |c| c.blue() > 0.9 && c.red() < 0.1 && c.green() < 0.1);
        assert_eq!(
            blue_mid,
            IntRect::new(30, 50, 70, 90),
            "offset_before={offset_before} mid-transition"
        );
    }
}

/// Interrupting an active `"A" -> "B"` transition with `defer("A")` applying both off-center scale
/// (`0.75` around `(1.0, 0.0)`) and offset (`(-40, 30)`), then cancelling back (`animate_to("B")`)
/// once the catch-up settled, renders at the same pixels on the hand-off frame.
#[test]
fn interrupted_transition_with_scale_and_offset_cancelled_back_has_no_pixel_jump() {
    let state = DeferredTransitionState::new("A");
    let scale = mutable_state_of(1.0f32);
    let offset = mutable_state_of(IntOffset::ZERO);
    let (held, s_read, o_read) = (state.clone(), scale.handle(), offset.handle());

    let transform = MutableContentTransform::new().initial_content_transform(move |it, _| {
        if held.pending_target_state() == Some("A") {
            it.set_transform_origin(origin(1.0, 0.0));
            it.set_scale(s_read.get());
            it.set_offset(o_read.get());
        }
    });

    let mut ui = deferred_rule(
        400,
        &state,
        |_| {
            slide_in_horizontally_with(tween(1000), |w| -w / 2)
                .together_with(slide_out_horizontally_with(tween(1000), |w| w / 2))
        },
        move |_| Some(transform.clone()),
        |scope, content, target| {
            let (scope, content) = (scope.clone(), content.clone());
            Box!(modifier = Modifier.fill_max_size(), move |_| {
                let shared = scope.remember_shared_content_state("box");
                let (x, y) = if target == "A" { (20, 40) } else { (180, 160) };
                Box!(
                    modifier = Modifier
                        .offset(x.dp(), y.dp())
                        .then(scope.shared_bounds(&shared, &content))
                        .size(100.dp())
                        .background(Color::RED)
                );
            });
        },
    );

    ui.wait_for_idle();
    ui.set_auto_advance(false);

    state.animate_to("B");
    ui.advance_time_by(Duration::from_millis(400));
    ui.wait_for_idle();

    state.defer("A");
    scale.set(0.75);
    offset.set(IntOffset::new(-40, 30));
    // Held until the catch-up has settled: a hand-off during it starts from the pivot of the
    // catching-up origin while the overlay draws at the manual one, as Compose's (D2100).
    for _ in 0..40 {
        ui.advance_time_by_frame();
        ui.wait_for_idle();
    }
    let red_before = bounds_of(&mut ui, |c| c.red() > 0.5 && c.green() < 0.1);
    assert_ne!(red_before, IntRect::ZERO);

    state.animate_to("B");
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    let red_after = bounds_of(&mut ui, |c| c.red() > 0.5 && c.green() < 0.1);
    assert!(
        near(red_before, red_after, 2),
        "expected no pixel jump on cancelled interruption: before={red_before:?}, after={red_after:?}"
    );

    ui.set_auto_advance(true);
    ui.wait_for_idle();
    let final_red = bounds_of(&mut ui, |c| c.red() > 0.9 && c.green() < 0.1);
    assert_eq!(final_red, IntRect::new(180, 160, 280, 260));
}
