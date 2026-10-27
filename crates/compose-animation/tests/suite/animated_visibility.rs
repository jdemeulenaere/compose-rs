//! `AnimatedVisibility` against Compose's: the size, offset, alpha and scale of the content
//! frame by frame along the given specs, the defaults, `None` transitions, interruptions, the
//! settle time, and the column and row defaults.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::{DrawOp, Optimizations};
use compose_ui::{Color, IntOffset, IntSize, Rect, TransformOrigin, UnitExt};

fn lerp(start: f32, stop: f32, fraction: f32) -> f32 {
    (1.0 - fraction) * start + fraction * stop
}

/// The bounds of the node tagged `tag`, in the root.
fn bounds(ui: &mut ComposeTestRule, tag: &str) -> Rect {
    ui.on_node_with_tag(tag).fetch_semantics_node().bounds
}

/// A rule showing a 100 × 100 box (tagged "box") in an `AnimatedVisibility` (tagged
/// "visibility") at the root's origin, hidden at first.
fn shown_box(
    enter: EnterTransition,
    exit: ExitTransition,
) -> (ComposeTestRule, OwnedState<bool>, Rc<Cell<bool>>) {
    use_optimizations(Optimizations::NONE);
    let visible = mutable_state_of(false);
    let handle = visible.handle();
    let disposed = Rc::new(Cell::new(false));
    let seen = disposed.clone();
    let ui = ComposeTestRule::with_manual_clock(move || {
        let seen = seen.clone();
        AnimatedVisibility!(
            handle.get(),
            modifier = Modifier.test_tag("visibility"),
            enter = enter.clone(),
            exit = exit.clone(),
            move |_| {
                let seen = seen.clone();
                Box!(
                    modifier = Modifier
                        .required_size((100.dp(), 100.dp()))
                        .background(Color::RED)
                        .test_tag("box"),
                    {
                        let seen = seen.clone();
                        DisposableEffect((), move |scope| {
                            seen.set(false);
                            let seen = seen.clone();
                            scope.on_dispose(move || seen.set(true))
                        });
                    }
                );
            }
        );
    });
    (ui, visible, disposed)
}

/// Expands in from a quarter of its width and half its height, from the bottom end; shrinks out
/// to a tenth and a fifth, towards the center start.
fn expand_and_shrink() -> (EnterTransition, ExitTransition) {
    (
        expand_in_with(
            tween(160).easing(LINEAR_OUT_SLOW_IN_EASING),
            Alignment::BOTTOM_END,
            true,
            |full| IntSize::new(full.width / 4, full.height / 2),
        ),
        shrink_out_with(
            tween(160).easing(FAST_OUT_SLOW_IN_EASING),
            Alignment::CENTER_START,
            true,
            |full| IntSize::new(full.width / 10, full.height / 5),
        ),
    )
}

/// Checks an expansion frame by frame, from play time 0.
fn check_expansion(ui: &mut ComposeTestRule) {
    for play in (0..=160).step_by(16) {
        let fraction = LINEAR_OUT_SLOW_IN_EASING.transform(play as f32 / 160.0);
        let (width, height) = (lerp(25.0, 100.0, fraction), lerp(50.0, 100.0, fraction));
        let outer = bounds(ui, "visibility");
        assert!((outer.width() - width).abs() <= 2.0, "at {play}: {outer:?}");
        assert!((outer.height() - height).abs() <= 2.0, "at {play}: {outer:?}");
        // From the bottom end: the box's bottom end at the animated size's.
        let inner = bounds(ui, "box");
        assert!((inner.left - (width - 100.0)).abs() <= 2.0, "at {play}: {inner:?}");
        assert!((inner.top - (height - 100.0)).abs() <= 2.0, "at {play}: {inner:?}");
        ui.advance_time_by(Duration::from_millis(16));
    }
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedVisibilityTest.kt, animateVisibilityExpandShrinkTest
#[test]
fn expanding_and_shrinking_animate_the_size_along_their_specs() {
    let (enter, exit) = expand_and_shrink();
    let (mut ui, visible, disposed) = shown_box(enter, exit);
    visible.set(true);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert!(!disposed.get());
    check_expansion(&mut ui);
    visible.set(false);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    // Up to the last frame before the end: at 160 ms the exit ends and the content leaves in
    // that frame (Compose's test reads the values it recorded before).
    for play in (0..160).step_by(16) {
        let fraction = FAST_OUT_SLOW_IN_EASING.transform(play as f32 / 160.0);
        let (width, height) = (lerp(100.0, 10.0, fraction), lerp(100.0, 20.0, fraction));
        let outer = bounds(&mut ui, "visibility");
        assert!((outer.width() - width).abs() <= 2.0, "at {play}: {outer:?}");
        assert!((outer.height() - height).abs() <= 2.0, "at {play}: {outer:?}");
        // Towards the center start.
        let inner = bounds(&mut ui, "box");
        assert!(inner.left.abs() <= 2.0, "at {play}: {inner:?}");
        assert!((inner.top - (height - 100.0) / 2.0).abs() <= 2.0, "at {play}: {inner:?}");
        ui.advance_time_by(Duration::from_millis(16));
    }
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert!(disposed.get(), "the content left once its exit ended");
    ui.set_auto_advance(false);
    // Shown again, the same as the first time.
    visible.set(true);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    check_expansion(&mut ui);
}

/// Checks a slide frame by frame from `(from_x, from_y)` to `(to_x, to_y)`, from play time 0, up
/// to `end` ms.
fn check_slide(
    ui: &mut ComposeTestRule,
    easing: Easing,
    from: (f32, f32),
    to: (f32, f32),
    end: u32,
) {
    for play in (0..=end).step_by(16) {
        let fraction = easing.transform(play as f32 / 160.0);
        let outer = bounds(ui, "visibility");
        assert_eq!((outer.width(), outer.height()), (100.0, 100.0), "at {play}");
        let inner = bounds(ui, "box");
        assert!((inner.left - lerp(from.0, to.0, fraction)).abs() <= 2.0, "at {play}: {inner:?}");
        assert!((inner.top - lerp(from.1, to.1, fraction)).abs() <= 2.0, "at {play}: {inner:?}");
        ui.advance_time_by(Duration::from_millis(16));
    }
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedVisibilityTest.kt, animateVisibilitySlideTest
#[test]
fn sliding_animates_the_offset_along_its_specs() {
    let enter = slide_in_with(tween(160).easing(LINEAR_OUT_SLOW_IN_EASING), |full| {
        IntOffset::new(full.width / 4, -full.height / 2)
    });
    let exit = slide_out_with(tween(160).easing(FAST_OUT_SLOW_IN_EASING), |full| {
        IntOffset::new(-full.width / 10, full.height / 5)
    });
    let (mut ui, visible, disposed) = shown_box(enter, exit);
    visible.set(true);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    check_slide(&mut ui, LINEAR_OUT_SLOW_IN_EASING, (25.0, -50.0), (0.0, 0.0), 160);
    visible.set(false);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    // Up to the last frame before the end, when the content leaves.
    check_slide(&mut ui, FAST_OUT_SLOW_IN_EASING, (0.0, 0.0), (-10.0, 20.0), 144);
    ui.advance_time_by_frame();
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert!(disposed.get());
    ui.set_auto_advance(false);
    visible.set(true);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    check_slide(&mut ui, LINEAR_OUT_SLOW_IN_EASING, (25.0, -50.0), (0.0, 0.0), 160);
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedVisibilityTest.kt, animateVisibilityContentSizeChangeTest
#[test]
fn a_visible_contents_size_change_shows_at_once() {
    let size = mutable_state_of(40.0f32);
    let visible = mutable_state_of(true);
    let (size_handle, visible_handle) = (size.handle(), visible.handle());
    let mut ui = compose_test(move || {
        AnimatedVisibility!(
            visible_handle.get(),
            modifier = Modifier.test_tag("visibility"),
            |_| {
                Box!(modifier = Modifier.size(size_handle.get().dp()));
            }
        );
    });
    let size_of = |ui: &mut ComposeTestRule| {
        let bounds = bounds(ui, "visibility");
        (bounds.width(), bounds.height())
    };
    assert_eq!(size_of(&mut ui), (40.0, 40.0));
    size.set(60.0);
    ui.wait_for_idle();
    assert_eq!(size_of(&mut ui), (60.0, 60.0));
    visible.set(false);
    ui.wait_for_idle();
    visible.set(true);
    ui.wait_for_idle();
    assert_eq!(size_of(&mut ui), (60.0, 60.0));
    size.set(30.0);
    ui.wait_for_idle();
    assert_eq!(size_of(&mut ui), (30.0, 30.0));
}

/// The alphas and scales of the layers drawn.
fn layers(ui: &mut ComposeTestRule) -> (Vec<f32>, Vec<f32>) {
    let mut alphas = Vec::new();
    let mut scales = Vec::new();
    for op in &ui.host().display_list().ops {
        match op {
            DrawOp::SaveLayerAlpha { alpha, .. } => alphas.push(*alpha),
            DrawOp::Transform { scale, .. } => scales.push(scale.0),
            _ => {}
        }
    }
    (alphas, scales)
}

/// The content's play time, read by its scope's transition.
type PlayTime = Rc<Cell<(EnterExitState, EnterExitState, i64)>>;

/// A box fading or scaling in and out along `enter` and `exit`, its transition's states and play
/// time reported.
fn reporting_box(
    enter: EnterTransition,
    exit: ExitTransition,
) -> (ComposeTestRule, OwnedState<bool>, PlayTime) {
    use_optimizations(Optimizations::NONE);
    let visible = mutable_state_of(false);
    let handle = visible.handle();
    let report: PlayTime =
        Rc::new(Cell::new((EnterExitState::PreEnter, EnterExitState::PreEnter, 0)));
    let seen = report.clone();
    let ui = ComposeTestRule::with_manual_clock(move || {
        let seen = seen.clone();
        AnimatedVisibility!(
            handle.get(),
            enter = enter.clone(),
            exit = exit.clone(),
            move |scope: AnimatedVisibilityScope| {
                let transition = scope.transition().clone();
                seen.set((
                    transition.current_state(),
                    transition.target_state(),
                    transition.play_time_nanos(),
                ));
                Box!(modifier = Modifier.size(20.dp()).background(Color::WHITE));
            }
        );
    });
    (ui, visible, report)
}

/// Runs frames until the transition settles, checking `value` against `expected(play time)`.
fn check_frames(
    ui: &mut ComposeTestRule,
    report: &PlayTime,
    value: impl Fn(&mut ComposeTestRule) -> f32,
    expected: impl Fn(i64) -> f32,
    settled: Option<f32>,
) {
    let mut last_play = -1;
    for _ in 0..100 {
        ui.advance_time_by_frame();
        let (current, target, play) = report.get();
        if current == target {
            assert_eq!(Some(value(ui)), settled);
            return;
        }
        if play == last_play {
            // An exit's end: the content left (its report stopped), as `settled` says.
            assert_eq!(settled, None, "the content left before its transition settled");
            return;
        }
        last_play = play;
        let millis = play / 1_000_000;
        // A layer whose alpha rounds to nothing draws nothing.
        if expected(millis) * 255.0 < 0.5 {
            continue;
        }
        assert!(
            (value(ui) - expected(millis)).abs() <= 0.01,
            "at {millis}: {} against {}",
            value(ui),
            expected(millis)
        );
    }
    panic!("never settled");
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedVisibilityTest.kt, animateVisibilityFadeTest
#[test]
fn fading_animates_the_alpha_along_its_specs() {
    let (mut ui, visible, report) = reporting_box(
        fade_in_with(tween(500).easing(FAST_OUT_LINEAR_IN_EASING), 0.0),
        fade_out_with(tween(300).easing(FAST_OUT_SLOW_IN_EASING), 0.0),
    );
    let alpha = |ui: &mut ComposeTestRule| layers(ui).0.first().copied().unwrap_or(1.0);
    visible.set(true);
    ui.advance_time_by_frame();
    check_frames(
        &mut ui,
        &report,
        alpha,
        |play| FAST_OUT_LINEAR_IN_EASING.transform((play as f32 / 500.0).min(1.0)),
        Some(1.0),
    );
    visible.set(false);
    ui.advance_time_by_frame();
    check_frames(
        &mut ui,
        &report,
        alpha,
        |play| 1.0 - FAST_OUT_SLOW_IN_EASING.transform((play as f32 / 300.0).min(1.0)),
        None,
    );
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedVisibilityTest.kt, animateVisibilityScaleTest
#[test]
fn scaling_animates_the_scale_along_its_specs() {
    let (mut ui, visible, report) = reporting_box(
        scale_in_with(tween(500).easing(FAST_OUT_LINEAR_IN_EASING), 0.0, TransformOrigin::CENTER),
        scale_out_with(tween(300).easing(FAST_OUT_SLOW_IN_EASING), 0.0, TransformOrigin::CENTER),
    );
    let scale = |ui: &mut ComposeTestRule| layers(ui).1.first().copied().unwrap_or(1.0);
    visible.set(true);
    ui.advance_time_by_frame();
    check_frames(
        &mut ui,
        &report,
        scale,
        |play| FAST_OUT_LINEAR_IN_EASING.transform((play as f32 / 500.0).min(1.0)),
        Some(1.0),
    );
    visible.set(false);
    ui.advance_time_by_frame();
    check_frames(
        &mut ui,
        &report,
        scale,
        |play| 1.0 - FAST_OUT_SLOW_IN_EASING.transform((play as f32 / 300.0).min(1.0)),
        None,
    );
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedVisibilityTest.kt, testScaleInAndScaleOutDefaultVisibilityThreshold
#[test]
fn the_default_scale_springs_threshold_is_two_thousandths() {
    let spring_threshold = |spec: AnimationSpec| match spec {
        AnimationSpec::Spring(spring) => spring.visibility_threshold,
        other => panic!("{other:?}"),
    };
    let threshold = Some(AnimationVector::from(0.002));
    assert_eq!(
        spring_threshold(scale_in().config().scale.as_ref().unwrap().animation_spec.clone()),
        threshold
    );
    assert_eq!(
        spring_threshold(scale_out().config().scale.as_ref().unwrap().animation_spec.clone()),
        threshold
    );
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedVisibilityTest.kt, testEnterTransitionNoneAndExitTransitionNone
#[test]
fn no_transitions_show_and_hide_at_the_next_frame() {
    use_optimizations(Optimizations::NONE);
    let state = MutableTransitionState::new(false);
    let given = state.clone();
    let disposed = Rc::new(Cell::new(false));
    let seen = disposed.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let seen = seen.clone();
        AnimatedVisibility!(
            given.clone(),
            modifier = Modifier.test_tag("visibility"),
            enter = EnterTransition::NONE,
            exit = ExitTransition::NONE,
            move |_| {
                let seen = seen.clone();
                Box!(modifier = Modifier.required_size((100.dp(), 100.dp())), {
                    let seen = seen.clone();
                    DisposableEffect((), move |scope| {
                        let seen = seen.clone();
                        scope.on_dispose(move || seen.set(true))
                    });
                });
            }
        );
    });
    ui.on_node_with_tag("visibility").assert_does_not_exist();
    state.set_target_state(true);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    let shown = bounds(&mut ui, "visibility");
    assert_eq!((shown.width(), shown.height()), (100.0, 100.0));
    assert!(!disposed.get());
    state.set_target_state(false);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert!(disposed.get());
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedVisibilityTest.kt, interruptedExitAnimationUsesCorrectTransition
#[test]
fn an_interrupting_exit_takes_the_exit_given_with_it() {
    use_optimizations(Optimizations::NONE);
    let visible = mutable_state_of(false);
    let handle = visible.handle();
    let duration = 16 * 20;
    let animation = move || tween(duration).easing(LINEAR_EASING);
    let mut ui = compose_test(move || {
        let shown = handle.get();
        AnimatedVisibility!(
            shown,
            enter = slide_in_horizontally_with(animation(), |_| 180),
            exit = if shown {
                slide_out_horizontally_with(animation(), |_| -200)
            } else {
                // Only this one applies.
                slide_out_horizontally_with(animation(), |_| 200)
            },
            |_| {
                Box!(
                    modifier =
                        Modifier.required_size(100.dp()).background(Color::RED).test_tag("box")
                );
            }
        );
    });
    ui.set_auto_advance(false);
    visible.set(true);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(u64::from(duration) / 2));
    assert_eq!((bounds(&mut ui, "box").left, bounds(&mut ui, "box").top), (90.0, 0.0));
    visible.set(false);
    ui.advance_time_by_frame();
    let at_interruption = bounds(&mut ui, "box").left;
    for _ in 0..3 {
        ui.advance_time_by_frame();
    }
    assert_eq!(bounds(&mut ui, "box").top, 0.0);
    assert!(
        bounds(&mut ui, "box").left > at_interruption,
        "{} after {at_interruption}",
        bounds(&mut ui, "box").left
    );
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedVisibilityTest.kt, testAnimationSettleExactTime
#[test]
fn content_without_an_exit_leaves_a_frame_after_the_change() {
    let state = MutableTransitionState::new(true);
    let given = state.clone();
    let present = Rc::new(Cell::new(false));
    let seen = present.clone();
    let mut ui = compose_test(move || {
        let seen = seen.clone();
        AnimatedVisibility!(
            given.clone(),
            enter = EnterTransition::NONE,
            exit = ExitTransition::NONE,
            move |_| {
                let seen = seen.clone();
                DisposableEffect((), move |scope| {
                    seen.set(true);
                    let seen = seen.clone();
                    scope.on_dispose(move || seen.set(false))
                });
            }
        );
    });
    assert!(present.get(), "the content is composed");
    ui.set_auto_advance(false);
    state.set_target_state(false);
    // It ends a frame after the change composes: a frame, and a millisecond (rounded up to
    // another frame).
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(1));
    assert!(!present.get(), "the content left");
}

/// The default transitions of the column and row overloads: along the column or the row.
#[test]
fn in_a_column_or_a_row_the_defaults_expand_along_it() {
    use_optimizations(Optimizations::NONE);
    let visible = mutable_state_of(false);
    let handle = visible.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        Column!(|column: ColumnScope| {
            AnimatedVisibility!(
                (column, handle.get()),
                modifier = Modifier.test_tag("column"),
                |_| {
                    Box!(modifier = Modifier.size(100.dp()));
                }
            );
        });
        Row!(modifier = Modifier.offset(0.dp(), 200.dp()), |row: RowScope| {
            AnimatedVisibility!((row, handle.get()), modifier = Modifier.test_tag("row"), |_| {
                Box!(modifier = Modifier.size(100.dp()));
            });
        });
    });
    visible.set(true);
    for _ in 0..4 {
        ui.advance_time_by_frame();
    }
    // Mid-way: the column's only grows in height, the row's in width.
    let column = bounds(&mut ui, "column");
    assert_eq!(column.width(), 100.0, "{column:?}");
    assert!(column.height() > 0.0 && column.height() < 100.0, "{column:?}");
    let row = bounds(&mut ui, "row");
    assert_eq!(row.height(), 100.0, "{row:?}");
    assert!(row.width() > 0.0 && row.width() < 100.0, "{row:?}");
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum TestState {
    State1,
    State2,
    State3,
}

thread_local! {
    static SHOWING: std::cell::RefCell<Option<Transition<TestState>>> =
        const { std::cell::RefCell::new(None) };
}

fn showing_state() -> TestState {
    SHOWING.with(|showing| showing.borrow().as_ref().unwrap().current_state())
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedVisibilityTest.kt, testTransitionExtensionAnimatedVisibility
#[test]
fn a_transitions_visibility_follows_its_states() {
    let state = mutable_state_of(TestState::State1);
    let handle = state.handle();
    let disposed = Rc::new(Cell::new(false));
    let seen = disposed.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let transition = update_transition(handle.get(), None);
        SHOWING.with(|showing| *showing.borrow_mut() = Some(transition.clone()));
        let seen = seen.clone();
        let linear = tween(100).easing(LINEAR_EASING);
        transition.AnimatedVisibility(
            // Only visible in State2.
            |state| *state == TestState::State2,
            Modifier.test_tag("content"),
            // Linear, or the target size is reached before the animation ends.
            expand_in_with(linear.clone(), Alignment::BOTTOM_END, true, |_| IntSize::ZERO),
            shrink_out_with(linear, Alignment::BOTTOM_END, true, |_| IntSize::ZERO),
            move |_| {
                let seen = seen.clone();
                Box!(modifier = Modifier.required_size(100.dp()), {
                    let seen = seen.clone();
                    DisposableEffect((), move |scope| {
                        let seen = seen.clone();
                        scope.on_dispose(move || seen.set(true))
                    });
                });
            },
        );
    });
    ui.wait_for_idle();
    ui.on_node_unmerged(has_test_tag("content")).assert_does_not_exist();
    state.set(TestState::State2);
    // Composed at the next frame (Compose's loop checks the width before, trivially below 100).
    ui.advance_time_by_frame();
    while showing_state() != TestState::State2 {
        assert!(bounds(&mut ui, "content").width() < 100.0);
        ui.advance_time_by_frame();
    }
    let shown = bounds(&mut ui, "content");
    assert_eq!((shown.width(), shown.height()), (100.0, 100.0));
    state.set(TestState::State3);
    while showing_state() != TestState::State3 {
        assert!(bounds(&mut ui, "content").width() > 0.0);
        assert!(!disposed.get());
        ui.advance_time_by_frame();
    }
    // Removed when its exit ends, never measured at 0.
    ui.on_node_unmerged(has_test_tag("content")).assert_does_not_exist();
    assert!(disposed.get());
}

/// The default slides come in from half the content's size: `slideInHorizontally()` from
/// `-width / 2`, `slideInVertically()` from `-height / 2`, as Compose's defaults.
#[test]
fn the_default_slides_start_half_way_out() {
    for (enter, expected) in
        [(slide_in_horizontally(), (-50.0, 0.0)), (slide_in_vertically(), (0.0, -50.0))]
    {
        let (mut ui, visible, _) = shown_box(enter, ExitTransition::NONE);
        visible.set(true);
        // The change composes; then the first frame of the slide, at its start.
        ui.advance_time_by_frame();
        ui.advance_time_by_frame();
        let at = bounds(&mut ui, "box");
        assert_eq!((at.left, at.top), expected);
    }
}

/// The clip rects of the display list.
fn clips(ui: &mut ComposeTestRule) -> Vec<Rect> {
    ui.host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::ClipRect(rect) => Some(*rect),
            _ => None,
        })
        .collect()
}

/// An expansion clips the content to the animated size, unless it's told not to.
#[test]
fn an_expansion_clips_to_its_animated_size_unless_told_not_to() {
    for clip in [true, false] {
        let enter =
            expand_in_with(tween(160).easing(LINEAR_EASING), Alignment::TOP_START, clip, |_| {
                IntSize::ZERO
            });
        let (mut ui, visible, _) = shown_box(enter, ExitTransition::NONE);
        visible.set(true);
        ui.advance_time_by_frame();
        ui.advance_time_by_frame();
        // 80 ms in: half the size.
        ui.advance_time_by(Duration::from_millis(80));
        let half =
            clips(&mut ui).into_iter().any(|rect| rect.width() == 50.0 && rect.height() == 50.0);
        assert_eq!(half, clip, "clip {clip}: {:?}", clips(&mut ui));
    }
}

/// A scale's origin is its pivot: scaled to a half around the bottom end, the content's top
/// start corner lands at the middle.
#[test]
fn a_scale_pivots_on_its_transform_origin() {
    let enter = scale_in_with(
        tween(160),
        0.5,
        TransformOrigin { pivot_fraction_x: 1.0, pivot_fraction_y: 1.0 },
    );
    let (mut ui, visible, _) = shown_box(enter, ExitTransition::NONE);
    visible.set(true);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    let transform = ui.host().display_list().ops.iter().find_map(|op| match op {
        DrawOp::Transform { pivot, scale, .. } => Some((*pivot, *scale)),
        _ => None,
    });
    let (pivot, scale) = transform.expect("a scale");
    assert_eq!(scale, (0.5, 0.5));
    // Where the content's (0, 0) lands, in its layer.
    let corner = (pivot.x + (0.0 - pivot.x) * scale.0, pivot.y + (0.0 - pivot.y) * scale.1);
    assert_eq!(corner, (50.0, 50.0));
}

/// `animate_enter_exit` animates a child on the content's transition, with the content's own
/// parts, and the content stays until the child's exit ends too.
#[test]
fn a_childs_own_enter_and_exit_play_with_the_content() {
    use_optimizations(Optimizations::NONE);
    let visible = mutable_state_of(true);
    let handle = visible.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        AnimatedVisibility!(
            handle.get(),
            enter = EnterTransition::NONE,
            exit = ExitTransition::NONE,
            move |scope: AnimatedVisibilityScope| {
                let linear = tween(160).easing(LINEAR_EASING);
                let child = scope.animate_enter_exit(
                    fade_in_with(linear.clone(), 0.0),
                    fade_out_with(linear, 0.0),
                    "child",
                );
                Box!(
                    modifier =
                        child.required_size(100.dp()).background(Color::RED).test_tag("child")
                );
            }
        );
    });
    visible.set(false);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    // Half-way through the child's exit: half its alpha, and still there.
    ui.advance_time_by(Duration::from_millis(80));
    let alpha = layers(&mut ui).0.first().copied().expect("a layer");
    assert!((alpha - 0.5).abs() < 0.01, "{alpha}");
    ui.on_node_unmerged(has_test_tag("child")).assert_exists();
    ui.advance_time_by(Duration::from_millis(160));
    ui.on_node_unmerged(has_test_tag("child")).assert_does_not_exist();
}
