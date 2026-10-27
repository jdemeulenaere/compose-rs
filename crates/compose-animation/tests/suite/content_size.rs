//! `Modifier.animate_content_size` against Compose's `animateContentSize`: the size frame by
//! frame along its spec, the finished listener, and the content placed by the alignment in
//! either direction.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, LayoutDirection, Rect, UnitExt};

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> Rect {
    ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_node().bounds
}

const FRAME: u32 = 16;
const DURATION: u32 = 10 * FRAME;

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimationModifierTest.kt, animateContentSizeTest
#[test]
fn a_content_size_animates_along_its_spec_and_reports_its_end() {
    let size = mutable_state_of((100.0f32, 400.0f32));
    let handle = size.handle();
    let ends = Rc::new(Cell::new(None::<(IntSize, IntSize)>));
    let heard = ends.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let heard = heard.clone();
        let (width, height) = handle.get();
        Box!(
            modifier = Modifier
                .test_tag("animated")
                .animate_content_size_with(
                    tween(DURATION).easing(LINEAR_OUT_SLOW_IN_EASING),
                    Alignment::TOP_START,
                    Some(move |start, end| heard.set(Some((start, end)))),
                )
                .required_size((width.dp(), height.dp()))
        );
    });
    size.set((150.0, 200.0));
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    for play in (0..=DURATION).step_by(FRAME as usize) {
        let fraction = LINEAR_OUT_SLOW_IN_EASING.transform(play as f32 / DURATION as f32);
        let animated = bounds(&mut ui, "animated");
        let (width, height) = (
            100.0 * (1.0 - fraction) + 150.0 * fraction,
            400.0 * (1.0 - fraction) + 200.0 * fraction,
        );
        assert!((animated.width() - width).abs() <= 1.0, "at {play}: {animated:?}");
        assert!((animated.height() - height).abs() <= 1.0, "at {play}: {animated:?}");
        if play == DURATION {
            assert_eq!(ends.get(), Some((IntSize::new(100, 400), IntSize::new(150, 200))));
        } else {
            assert_eq!(ends.get(), None, "at {play}");
        }
        ui.advance_time_by(Duration::from_millis(u64::from(FRAME)));
    }
}

#[composable]
fn Aligned(size: MutableState<(f32, f32)>, alignment: Alignment, tag: &'static str) {
    let (width, height) = size.get();
    Box!(
        modifier = Modifier.test_tag(tag).animate_content_size_with(
            tween(DURATION).easing(LINEAR_OUT_SLOW_IN_EASING),
            alignment,
            None::<fn(_, _)>,
        ),
        {
            Box!(modifier = Modifier.required_size((width.dp(), height.dp())).test_tag("content"));
        }
    );
}

/// Each alignment's content, in a box of its own, frame by frame.
fn check_alignment(direction: LayoutDirection) {
    for alignment in [Alignment::TOP_START, Alignment::CENTER, Alignment::BOTTOM_END] {
        let size = mutable_state_of((100.0f32, 400.0f32));
        let handle = size.handle();
        let mut ui =
            ComposeTestRule::with_manual_clock(move || Aligned(handle, alignment, "animated"));
        ui.host().set_layout_direction(direction);
        ui.wait_for_idle();
        size.set((150.0, 200.0));
        ui.advance_time_by_frame();
        ui.advance_time_by_frame();
        for play in (0..=DURATION).step_by(FRAME as usize) {
            let fraction = LINEAR_OUT_SLOW_IN_EASING.transform(play as f32 / DURATION as f32);
            let space = IntSize::new(
                (100.0 * (1.0 - fraction) + 150.0 * fraction).round() as i32,
                (400.0 * (1.0 - fraction) + 200.0 * fraction).round() as i32,
            );
            let expected = alignment.align(IntSize::new(150, 200), space, direction);
            let (outer, inner) = (bounds(&mut ui, "animated"), bounds(&mut ui, "content"));
            let at = (inner.left - outer.left, inner.top - outer.top);
            assert!((at.0 - expected.x as f32).abs() <= 1.0, "{alignment:?} at {play}: {at:?}");
            assert!((at.1 - expected.y as f32).abs() <= 1.0, "{alignment:?} at {play}: {at:?}");
            ui.advance_time_by(Duration::from_millis(u64::from(FRAME)));
        }
    }
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimationModifierTest.kt, testAlignmentInAnimateContentSize_underLtr
#[test]
fn a_content_size_places_its_content_by_its_alignment_left_to_right() {
    check_alignment(LayoutDirection::Ltr);
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimationModifierTest.kt, testAlignmentInAnimateContentSize_underRtl
#[test]
fn a_content_size_places_its_content_by_its_alignment_right_to_left() {
    check_alignment(LayoutDirection::Rtl);
}

/// A box animating its content size, in reusable content keyed by `key` inside a host `active`
/// says, its listener counting the animations that finish.
#[composable]
fn ReusedAnimated(
    active: MutableState<bool>,
    key: MutableState<u32>,
    size: MutableState<f32>,
    finished: Rc<Cell<u32>>,
) {
    let finished = finished.clone();
    ReusableContentHost(active.get(), move || {
        let finished = finished.clone();
        ReusableContent(key.get(), move || {
            let finished = finished.clone();
            let side = size.get();
            Box!(
                modifier = Modifier
                    .test_tag("animated")
                    .animate_content_size_with(
                        tween(DURATION).easing(LINEAR_OUT_SLOW_IN_EASING),
                        Alignment::TOP_START,
                        Some(move |_, _| finished.set(finished.get() + 1)),
                    )
                    .required_size((side.dp(), side.dp()))
            );
        });
    });
}

/// An animation interrupted by reuse (`reuse`) or deactivation, then reactivation
/// (`deactivate`): the reused node takes the new content's size at once, as Compose's
/// `animData = null` on reset, and the cancelled animation tells its listener nothing (its scope
/// cancelled at the detach, D1153).
fn content_size_interrupted(deactivate: bool) {
    let (active, key, size) =
        (mutable_state_of(true), mutable_state_of(0), mutable_state_of(100.0));
    let finished = Rc::new(Cell::new(0));
    let (a, k, s, f) = (active.handle(), key.handle(), size.handle(), finished.clone());
    let mut ui = ComposeTestRule::with_manual_clock(move || ReusedAnimated(a, k, s, f.clone()));
    size.set(200.0);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(u64::from(3 * FRAME)));
    let midway = bounds(&mut ui, "animated");
    assert!(midway.width() > 100.0 && midway.width() < 200.0, "animating: {midway:?}");
    if deactivate {
        active.set(false);
        ui.advance_time_by_frame();
        active.set(true);
    }
    key.set(1);
    size.set(50.0);
    ui.advance_time_by_frame();
    let reused = bounds(&mut ui, "animated");
    assert_eq!((reused.width(), reused.height()), (50.0, 50.0), "the new size at once");
    ui.advance_time_by(Duration::from_millis(u64::from(2 * DURATION)));
    assert_eq!(finished.get(), 0, "the interrupted animation told nothing");
}

#[test]
fn a_content_size_animation_interrupted_by_reuse_takes_the_new_size_at_once() {
    content_size_interrupted(false);
}

#[test]
fn a_content_size_animation_interrupted_by_deactivation_takes_the_new_size_at_once() {
    content_size_interrupted(true);
}
